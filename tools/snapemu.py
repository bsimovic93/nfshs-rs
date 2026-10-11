"""Run one game function on a RAM snapshot taken from DuckStation (see snap.py) and compare
the result with the real post-call state. Logs every memory load/store outside the stack.

usage: snapemu.py snap.pkl
"""
import pickle, struct, sys, collections

RAM = 0x200000
SCRATCH_BASE, SCRATCH_SIZE = 0x1F800000, 0x400


class Mem:
    def __init__(self, ram, scratch):
        self.ram = bytearray(ram)
        self.scratch = bytearray(scratch)
        self.loads = collections.Counter()
        self.stores = collections.Counter()
        self.track = True

    io_log = None  # set to a list to trap hardware I/O instead of raising
    cur_func = 0

    def _map(self, addr):
        a = addr & 0x1FFFFFFF
        if SCRATCH_BASE <= a < SCRATCH_BASE + SCRATCH_SIZE:
            return self.scratch, a - SCRATCH_BASE
        if a < 0x800000:
            return self.ram, a & (RAM - 1)
        raise IndexError(f'unmapped access {addr:#x}')

    def _io(self, addr, kind, v=0):
        a = addr & 0x1FFFFFFF
        if self.io_log is not None and (0x1F801000 <= a < 0x1F803000 or a >= 0x1FC00000):
            self.io_log.append((kind, a, v, self.cur_func))
            return True
        return False

    def load(self, addr, n, signed=False):
        if self._io(addr, 'r'):
            return 0
        buf, o = self._map(addr)
        if self.track:
            self.loads[addr & ~3] += 1
        v = int.from_bytes(buf[o:o + n], 'little', signed=signed)
        return v & 0xFFFFFFFF

    def store(self, addr, n, v):
        if self._io(addr, 'w', v):
            return
        buf, o = self._map(addr)
        if self.track:
            self.stores[addr & ~3] += 1
        buf[o:o + n] = (v & ((1 << (8 * n)) - 1)).to_bytes(n, 'little')


def s32(v):
    return v - (1 << 32) if v & 0x80000000 else v


COP0 = [0] * 32
HLE = {}  # addr -> fn(r, mem) ; executed instead of the function, then returns to r[31]
BIOS = None  # fn(r, mem, fn_number) for A0 calls


def run(mem, regs, entry, ret, max_steps=5_000_000):
    r = list(regs[:32])
    hi, lo = regs[34], regs[33]
    pc, npc = entry, entry + 4
    steps = 0
    stack = [entry]
    calls = collections.Counter()
    while pc != ret:
        if pc in HLE:
            HLE[pc](r, mem)
            if len(stack) > 1 and stack[-1] == pc:
                stack.pop(); mem.cur_func = stack[-1]
            pc, npc = r[31], (r[31] + 4) & 0xFFFFFFFF
            continue
        if (pc & 0x1FFFFFFF) in (0xA0, 0xB0, 0xC0):
            handled = (pc & 0xFF) == 0xA0 and BIOS is not None and BIOS(r, mem, r[9])
            if not handled:
                if mem.io_log is not None:
                    mem.io_log.append(('bios', pc & 0xFF, r[9], stack[-1]))
                r[2] = 0
            pc, npc = r[31], r[31] + 4
            continue
        steps += 1
        if steps > max_steps:
            raise RuntimeError('step limit')
        w = mem.load(pc, 4) if False else int.from_bytes(mem.ram[(pc & 0x1FFFFF):(pc & 0x1FFFFF) + 4], 'little')
        op = w >> 26
        rs, rt = (w >> 21) & 31, (w >> 16) & 31
        imm = w & 0xFFFF
        simm = imm - 0x10000 if imm & 0x8000 else imm
        nnpc = (npc + 4) & 0xFFFFFFFF
        if op == 0:
            rd, sh, fn = (w >> 11) & 31, (w >> 6) & 31, w & 63
            v = None
            if fn == 0: v = (r[rt] << sh) & 0xFFFFFFFF
            elif fn == 2: v = r[rt] >> sh
            elif fn == 3: v = (s32(r[rt]) >> sh) & 0xFFFFFFFF
            elif fn == 4: v = (r[rt] << (r[rs] & 31)) & 0xFFFFFFFF
            elif fn == 6: v = r[rt] >> (r[rs] & 31)
            elif fn == 7: v = (s32(r[rt]) >> (r[rs] & 31)) & 0xFFFFFFFF
            elif fn == 8:
                nnpc = r[rs]
                if rs == 31 and len(stack) > 1:
                    stack.pop(); mem.cur_func = stack[-1]
            elif fn == 9:
                v = (npc + 4) & 0xFFFFFFFF; nnpc = r[rs]
                calls[nnpc] += 1; stack.append(nnpc); mem.cur_func = nnpc
            elif fn == 16: v = hi
            elif fn == 18: v = lo
            elif fn == 17: hi = r[rs]
            elif fn == 19: lo = r[rs]
            elif fn == 24:
                p = (s32(r[rs]) * s32(r[rt])) & 0xFFFFFFFFFFFFFFFF
                lo, hi = p & 0xFFFFFFFF, p >> 32
            elif fn == 25:
                p = r[rs] * r[rt]
                lo, hi = p & 0xFFFFFFFF, p >> 32
            elif fn == 26:
                a, b = s32(r[rs]), s32(r[rt])
                if b:
                    q = abs(a) // abs(b) * (1 if (a < 0) == (b < 0) else -1)
                    lo, hi = q & 0xFFFFFFFF, (a - q * b) & 0xFFFFFFFF
                else:
                    lo, hi = (1 if a < 0 else 0xFFFFFFFF), a & 0xFFFFFFFF
            elif fn == 27:
                if r[rt]:
                    lo, hi = r[rs] // r[rt], r[rs] % r[rt]
                else:
                    lo, hi = 0xFFFFFFFF, r[rs]
            elif fn in (32, 33): v = (r[rs] + r[rt]) & 0xFFFFFFFF
            elif fn in (34, 35): v = (r[rs] - r[rt]) & 0xFFFFFFFF
            elif fn == 36: v = r[rs] & r[rt]
            elif fn == 37: v = r[rs] | r[rt]
            elif fn == 38: v = r[rs] ^ r[rt]
            elif fn == 39: v = ~(r[rs] | r[rt]) & 0xFFFFFFFF
            elif fn == 42: v = int(s32(r[rs]) < s32(r[rt]))
            elif fn == 43: v = int(r[rs] < r[rt])
            else: raise RuntimeError(f'special {fn} at {pc:#x}')
            if v is not None and rd: r[rd] = v
        elif op == 1:
            a = s32(r[rs])
            if rt & 16: r[31] = (npc + 4) & 0xFFFFFFFF
            if (rt & 1 and a >= 0) or (not rt & 1 and a < 0): nnpc = (npc + simm * 4) & 0xFFFFFFFF
        elif op == 2: nnpc = (npc & 0xF0000000) | ((w & 0x3FFFFFF) << 2)
        elif op == 3:
            r[31] = (npc + 4) & 0xFFFFFFFF
            nnpc = (npc & 0xF0000000) | ((w & 0x3FFFFFF) << 2)
            calls[nnpc] += 1
            stack.append(nnpc); mem.cur_func = nnpc
        elif op == 4:
            if r[rs] == r[rt]: nnpc = (npc + simm * 4) & 0xFFFFFFFF
        elif op == 5:
            if r[rs] != r[rt]: nnpc = (npc + simm * 4) & 0xFFFFFFFF
        elif op == 6:
            if s32(r[rs]) <= 0: nnpc = (npc + simm * 4) & 0xFFFFFFFF
        elif op == 7:
            if s32(r[rs]) > 0: nnpc = (npc + simm * 4) & 0xFFFFFFFF
        elif op in (8, 9):
            if rt: r[rt] = (r[rs] + simm) & 0xFFFFFFFF
        elif op == 10:
            if rt: r[rt] = int(s32(r[rs]) < simm)
        elif op == 11:
            if rt: r[rt] = int(r[rs] < (simm & 0xFFFFFFFF))
        elif op == 12:
            if rt: r[rt] = r[rs] & imm
        elif op == 13:
            if rt: r[rt] = r[rs] | imm
        elif op == 14:
            if rt: r[rt] = r[rs] ^ imm
        elif op == 15:
            if rt: r[rt] = imm << 16
        elif op == 16:
            if rs == 0:
                if rt: r[rt] = COP0[(w >> 11) & 31]
            elif rs == 4:
                COP0[(w >> 11) & 31] = r[rt]
            elif rs == 16 and (w & 63) == 16:
                pass  # rfe
            else:
                raise RuntimeError(f'cop0 {w:#010x} at {pc:#x}')
        elif op >= 32:
            addr = (r[rs] + simm) & 0xFFFFFFFF
            if op == 32: v = mem.load(addr, 1, True)
            elif op == 33: v = mem.load(addr, 2, True)
            elif op == 35: v = mem.load(addr, 4)
            elif op == 36: v = mem.load(addr, 1)
            elif op == 37: v = mem.load(addr, 2)
            elif op in (34, 38):
                al = addr & ~3
                word = mem.load(al, 4)
                k = addr & 3
                cur = r[rt]
                if op == 34:
                    sh = (3 - k) * 8
                    v = (cur & ((0xFFFFFFFF >> (32 - sh)) if sh else 0)) | ((word << sh) & 0xFFFFFFFF)
                else:
                    sh = k * 8
                    v = (cur & (((0xFFFFFFFF << (32 - sh)) & 0xFFFFFFFF) if sh else 0)) | (word >> sh)
            elif op == 40: mem.store(addr, 1, r[rt]); v = None
            elif op == 41: mem.store(addr, 2, r[rt]); v = None
            elif op == 43: mem.store(addr, 4, r[rt]); v = None
            elif op in (42, 46):
                al = addr & ~3
                word = mem.load(al, 4)
                k = addr & 3
                if op == 42:
                    sh = (3 - k) * 8
                    word = (word & (((0xFFFFFFFF << (32 - sh)) & 0xFFFFFFFF) if sh else 0)) | (r[rt] >> sh)
                else:
                    sh = k * 8
                    word = (word & ((0xFFFFFFFF >> (32 - sh)) if sh else 0)) | ((r[rt] << sh) & 0xFFFFFFFF)
                mem.store(al, 4, word); v = None
            else:
                raise RuntimeError(f'mem op {op} at {pc:#x}')
            if op < 40 and rt:
                r[rt] = v & 0xFFFFFFFF
        else:
            raise RuntimeError(f'op {op} at {pc:#x} ({w:#010x})')
        pc, npc = npc, nnpc
    run.calls = calls
    return r, steps


def main(path):
    snap = pickle.load(open(path, 'rb'))
    f = snap['func']
    b, a = snap['before'], snap['after']
    mem = Mem(b['ram'], b['scratch'])
    regs, steps = run(mem, b['regs'], f, b['regs'][31])
    print(f'{f:#x}: {steps} instructions')
    # compare RAM
    diffs = [i for i in range(0, RAM, 4) if mem.ram[i:i + 4] != a['ram'][i:i + 4]]
    sdiffs = [i for i in range(0, SCRATCH_SIZE, 4) if mem.scratch[i:i + 4] != a['scratch'][i:i + 4]]
    print('ram words differing from real result:', len(diffs), [hex(0x80000000 + i) for i in diffs[:20]])
    print('scratch words differing:', len(sdiffs), [hex(SCRATCH_BASE + i) for i in sdiffs[:10]])
    print('v0', hex(regs[2]), 'real', hex(a['regs'][2]))
    return snap, mem


if __name__ == '__main__':
    snap, mem = main(sys.argv[1])
    sp = snap['before']['regs'][29]
    def region(x):
        x &= 0x1FFFFFFF
        if 0x1F800000 <= x < 0x1F800400: return 'scratch'
        return 'ram'
    car = 0x801C19A8
    print('\nloads outside car struct and stack:')
    outside = sorted(x for x in mem.loads if not (car <= x < car + 0x8F0) and region(x) == 'ram' and not (0x80010000 <= x < 0x8013F800))
    for x in outside:
        print(f'  {x:#x} x{mem.loads[x]}')
    print('data-section loads (exe image):', len([x for x in mem.loads if 0x80010000 <= x < 0x8013F800]))
    print('car fields read:', sorted(hex(x - car) for x in mem.loads if car <= x < car + 0x8F0))
    print('car fields written:', sorted(hex(x - car) for x in mem.stores if car <= x < car + 0x8F0))
