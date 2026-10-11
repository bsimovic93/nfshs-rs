"""Tiny MIPS-I interpreter to run self-contained routines from a PS-X EXE (ground truth for RE).

usage: mipsemu.py <exe> <func_addr> <infile> <outfile>
Runs func(a0=src, a1=dst) and writes dst[0:v0] (v0 = returned size, or max written).
"""
import struct, sys

MEM_SIZE = 8 << 20


class Cpu:
    def __init__(self, exe_path):
        d = open(exe_path, 'rb').read()
        pc0, gp0, taddr, tsize = struct.unpack_from('<IIII', d, 0x10)
        self.mem = bytearray(MEM_SIZE)
        off = taddr & 0x1fffffff
        self.mem[off:off + tsize] = d[0x800:0x800 + tsize]
        self.r = [0] * 32
        self.hi = self.lo = 0
        self.steps = 0
        self.max_write = 0

    def a(self, addr):
        return addr & 0x1fffffff

    def lw(self, addr):
        return struct.unpack_from('<I', self.mem, self.a(addr))[0]

    def run(self, pc, args, ret=0x8FFF0000 & 0x9fffffff):
        r = self.r
        for i, v in enumerate(args):
            r[4 + i] = v
        r[29] = 0x807FFF00
        r[31] = 0x807FFFF0  # sentinel return address
        sentinel = r[31]
        mem = self.mem
        npc = pc + 4
        unpack_from = struct.unpack_from
        pack_into = struct.pack_into
        while True:
            if pc == sentinel:
                return r[2]
            w = unpack_from('<I', mem, pc & 0x1fffffff)[0]
            self.steps += 1
            op = w >> 26
            rs = (w >> 21) & 31
            rt = (w >> 16) & 31
            imm = w & 0xffff
            simm = imm - 0x10000 if imm & 0x8000 else imm
            nnpc = npc + 4
            if op == 0:
                rd = (w >> 11) & 31
                sh = (w >> 6) & 31
                fn = w & 63
                v = None
                if fn == 0: v = (r[rt] << sh) & 0xffffffff
                elif fn == 2: v = r[rt] >> sh
                elif fn == 3: v = ((r[rt] - (1 << 32) if r[rt] & 0x80000000 else r[rt]) >> sh) & 0xffffffff
                elif fn == 4: v = (r[rt] << (r[rs] & 31)) & 0xffffffff
                elif fn == 6: v = r[rt] >> (r[rs] & 31)
                elif fn == 7: v = ((r[rt] - (1 << 32) if r[rt] & 0x80000000 else r[rt]) >> (r[rs] & 31)) & 0xffffffff
                elif fn == 8: nnpc = r[rs]
                elif fn == 9: v = npc + 4; nnpc = r[rs]
                elif fn == 16: v = self.hi
                elif fn == 18: v = self.lo
                elif fn == 17: self.hi = r[rs]
                elif fn == 19: self.lo = r[rs]
                elif fn == 24:
                    a = r[rs] - (1 << 32) if r[rs] & 0x80000000 else r[rs]
                    b = r[rt] - (1 << 32) if r[rt] & 0x80000000 else r[rt]
                    p = (a * b) & 0xffffffffffffffff
                    self.lo, self.hi = p & 0xffffffff, p >> 32
                elif fn == 25:
                    p = r[rs] * r[rt]
                    self.lo, self.hi = p & 0xffffffff, p >> 32
                elif fn == 26:
                    a = r[rs] - (1 << 32) if r[rs] & 0x80000000 else r[rs]
                    b = r[rt] - (1 << 32) if r[rt] & 0x80000000 else r[rt]
                    if b:
                        q = abs(a) // abs(b) * (1 if (a < 0) == (b < 0) else -1)
                        self.lo, self.hi = q & 0xffffffff, (a - q * b) & 0xffffffff
                elif fn == 27:
                    if r[rt]:
                        self.lo, self.hi = r[rs] // r[rt], r[rs] % r[rt]
                elif fn in (32, 33): v = (r[rs] + r[rt]) & 0xffffffff
                elif fn in (34, 35): v = (r[rs] - r[rt]) & 0xffffffff
                elif fn == 36: v = r[rs] & r[rt]
                elif fn == 37: v = r[rs] | r[rt]
                elif fn == 38: v = r[rs] ^ r[rt]
                elif fn == 39: v = ~(r[rs] | r[rt]) & 0xffffffff
                elif fn == 42:
                    a = r[rs] - (1 << 32) if r[rs] & 0x80000000 else r[rs]
                    b = r[rt] - (1 << 32) if r[rt] & 0x80000000 else r[rt]
                    v = int(a < b)
                elif fn == 43: v = int(r[rs] < r[rt])
                else:
                    raise RuntimeError(f'unimpl special {fn} at {pc:#x}')
                if v is not None and rd:
                    r[rd] = v
            elif op == 1:
                a = r[rs] - (1 << 32) if r[rs] & 0x80000000 else r[rs]
                if rt & 16:
                    r[31] = npc + 4
                if (rt & 1 and a >= 0) or (not rt & 1 and a < 0):
                    nnpc = (npc + simm * 4) & 0xffffffff
            elif op == 2:
                nnpc = (npc & 0xf0000000) | ((w & 0x3ffffff) << 2)
            elif op == 3:
                r[31] = npc + 4
                nnpc = (npc & 0xf0000000) | ((w & 0x3ffffff) << 2)
            elif op == 4:
                if r[rs] == r[rt]: nnpc = (npc + simm * 4) & 0xffffffff
            elif op == 5:
                if r[rs] != r[rt]: nnpc = (npc + simm * 4) & 0xffffffff
            elif op == 6:
                a = r[rs] - (1 << 32) if r[rs] & 0x80000000 else r[rs]
                if a <= 0: nnpc = (npc + simm * 4) & 0xffffffff
            elif op == 7:
                a = r[rs] - (1 << 32) if r[rs] & 0x80000000 else r[rs]
                if a > 0: nnpc = (npc + simm * 4) & 0xffffffff
            elif op in (8, 9):
                if rt: r[rt] = (r[rs] + simm) & 0xffffffff
            elif op == 10:
                a = r[rs] - (1 << 32) if r[rs] & 0x80000000 else r[rs]
                if rt: r[rt] = int(a < simm)
            elif op == 11:
                if rt: r[rt] = int(r[rs] < (simm & 0xffffffff))
            elif op == 12:
                if rt: r[rt] = r[rs] & imm
            elif op == 13:
                if rt: r[rt] = r[rs] | imm
            elif op == 14:
                if rt: r[rt] = r[rs] ^ imm
            elif op == 15:
                if rt: r[rt] = imm << 16
            elif op >= 32:
                addr = ((r[rs] + simm) & 0x1fffffff)
                if op == 32: v = mem[addr]; v = v - 256 if v & 128 else v; r[rt] = v & 0xffffffff if rt else 0
                elif op == 36:
                    if rt: r[rt] = mem[addr]
                elif op == 33:
                    v = unpack_from('<h', mem, addr)[0]
                    if rt: r[rt] = v & 0xffffffff
                elif op == 37:
                    if rt: r[rt] = unpack_from('<H', mem, addr)[0]
                elif op == 35:
                    if rt: r[rt] = unpack_from('<I', mem, addr)[0]
                elif op == 34 or op == 38:  # lwl / lwr
                    al = addr & ~3
                    word = unpack_from('<I', mem, al)[0]
                    k = addr & 3
                    cur = r[rt]
                    if op == 34:
                        sh = (3 - k) * 8
                        mask = (0xffffffff >> (32 - sh)) if sh else 0
                        v = (cur & mask) | ((word << sh) & 0xffffffff)
                    else:
                        sh = k * 8
                        mask = (0xffffffff << (32 - sh)) & 0xffffffff if sh else 0
                        v = (cur & mask) | (word >> sh)
                    if rt: r[rt] = v
                elif op == 40:
                    mem[addr] = r[rt] & 0xff; self.max_write = max(self.max_write, addr + 1)
                elif op == 41:
                    pack_into('<H', mem, addr, r[rt] & 0xffff); self.max_write = max(self.max_write, addr + 2)
                elif op == 43:
                    pack_into('<I', mem, addr, r[rt]); self.max_write = max(self.max_write, addr + 4)
                elif op in (42, 46):  # swl / swr
                    al = addr & ~3
                    word = unpack_from('<I', mem, al)[0]
                    k = addr & 3
                    if op == 42:
                        sh = (3 - k) * 8
                        keep = (0xffffffff << (32 - sh)) & 0xffffffff if sh else 0
                        word = (word & keep) | (r[rt] >> sh)
                    else:
                        sh = k * 8
                        keep = (0xffffffff >> (32 - sh)) if sh else 0
                        word = (word & keep) | ((r[rt] << sh) & 0xffffffff)
                    pack_into('<I', mem, al, word)
                else:
                    raise RuntimeError(f'unimpl mem op {op} at {pc:#x}')
            else:
                raise RuntimeError(f'unimpl op {op} at {pc:#x} ({w:#010x})')
            pc, npc = npc, nnpc


def decompress(exe, func, data):
    cpu = Cpu(exe)
    src, dst = 0x80300000, 0x80500000
    cpu.mem[src & 0x1fffffff:(src & 0x1fffffff) + len(data)] = data
    v0 = cpu.run(func, [src, dst])
    n = v0 if 0 < v0 < 0x200000 else cpu.max_write - (dst & 0x1fffffff)
    return bytes(cpu.mem[dst & 0x1fffffff:(dst & 0x1fffffff) + n]), cpu.steps


if __name__ == '__main__':
    exe, func, inp, outp = sys.argv[1], int(sys.argv[2], 16), sys.argv[3], sys.argv[4]
    out, steps = decompress(exe, func, open(inp, 'rb').read())
    open(outp, 'wb').write(out)
    print(f'{inp}: {len(out)} bytes, {steps} instructions')
