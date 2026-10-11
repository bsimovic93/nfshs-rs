"""Minimal MIPS-I (R3000A) disassembler + xref helper for PS-X EXE analysis.

usage:
  mips.py dis  <exe> <addr> [count]       disassemble
  mips.py xref <exe> <addr>               find lui/addiu-style references and jal calls to addr
  mips.py func <exe> <addr>               disassemble whole function containing addr (heuristic)
"""
import struct, sys

REG = ['zero','at','v0','v1','a0','a1','a2','a3','t0','t1','t2','t3','t4','t5','t6','t7',
       's0','s1','s2','s3','s4','s5','s6','s7','t8','t9','k0','k1','gp','sp','fp','ra']


class Exe:
    def __init__(self, path):
        d = open(path, 'rb').read()
        self.pc0, _, self.base, size = struct.unpack_from('<IIII', d, 0x10)
        self.data = d[0x800:0x800 + size]
        self.end = self.base + len(self.data)

    def word(self, a):
        return struct.unpack_from('<I', self.data, a - self.base)[0]

    def valid(self, a):
        return self.base <= a < self.end

    def cstr(self, a):
        if not self.valid(a):
            return None
        o = a - self.base
        e = self.data.find(b'\0', o, o + 128)
        if e <= o:
            return None
        s = self.data[o:e]
        if all(32 <= c < 127 or c in (9, 10, 13) for c in s) and len(s) >= 3:
            return s.decode()
        return None


def sx16(v):
    return v - 0x10000 if v & 0x8000 else v


def dis1(w, pc):
    op = w >> 26
    rs, rt, rd = (w >> 21) & 31, (w >> 16) & 31, (w >> 11) & 31
    sh, fn = (w >> 6) & 31, w & 63
    imm = w & 0xffff
    simm = sx16(imm)
    R = REG
    if w == 0:
        return 'nop'
    if op == 0:
        names = {0: 'sll', 2: 'srl', 3: 'sra'}
        if fn in names:
            return f'{names[fn]} {R[rd]}, {R[rt]}, {sh}'
        names = {4: 'sllv', 6: 'srlv', 7: 'srav'}
        if fn in names:
            return f'{names[fn]} {R[rd]}, {R[rt]}, {R[rs]}'
        if fn == 8:
            return f'jr {R[rs]}'
        if fn == 9:
            return f'jalr {R[rd]}, {R[rs]}'
        if fn == 12:
            return 'syscall'
        if fn == 13:
            return 'break'
        names = {16: 'mfhi', 18: 'mflo'}
        if fn in names:
            return f'{names[fn]} {R[rd]}'
        names = {17: 'mthi', 19: 'mtlo'}
        if fn in names:
            return f'{names[fn]} {R[rs]}'
        names = {24: 'mult', 25: 'multu', 26: 'div', 27: 'divu'}
        if fn in names:
            return f'{names[fn]} {R[rs]}, {R[rt]}'
        names = {32: 'add', 33: 'addu', 34: 'sub', 35: 'subu', 36: 'and', 37: 'or', 38: 'xor', 39: 'nor', 42: 'slt', 43: 'sltu'}
        if fn in names:
            if fn == 33 and rt == 0:
                return f'move {R[rd]}, {R[rs]}'
            return f'{names[fn]} {R[rd]}, {R[rs]}, {R[rt]}'
        return f'.word {w:#010x}'
    if op == 1:
        n = {0: 'bltz', 1: 'bgez', 16: 'bltzal', 17: 'bgezal'}.get(rt, '?regimm')
        return f'{n} {R[rs]}, {pc + 4 + simm * 4:#x}'
    if op in (2, 3):
        t = (pc & 0xf0000000) | ((w & 0x3ffffff) << 2)
        return f"{'j' if op == 2 else 'jal'} {t:#x}"
    if op in (4, 5):
        n = 'beq' if op == 4 else 'bne'
        if op == 4 and rs == 0 and rt == 0:
            return f'b {pc + 4 + simm * 4:#x}'
        return f'{n} {R[rs]}, {R[rt]}, {pc + 4 + simm * 4:#x}'
    if op in (6, 7):
        return f"{'blez' if op == 6 else 'bgtz'} {R[rs]}, {pc + 4 + simm * 4:#x}"
    names = {8: 'addi', 9: 'addiu', 10: 'slti', 11: 'sltiu'}
    if op in names:
        return f'{names[op]} {R[rt]}, {R[rs]}, {simm}'
    names = {12: 'andi', 13: 'ori', 14: 'xori'}
    if op in names:
        return f'{names[op]} {R[rt]}, {R[rs]}, {imm:#x}'
    if op == 15:
        return f'lui {R[rt]}, {imm:#x}'
    if op in (16, 18):
        cop = op - 16
        if rs == 0:
            return f'mfc{cop} {R[rt]}, ${rd}'
        if rs == 2:
            return f'cfc{cop} {R[rt]}, ${rd}'
        if rs == 4:
            return f'mtc{cop} {R[rt]}, ${rd}'
        if rs == 6:
            return f'ctc{cop} {R[rt]}, ${rd}'
        if rs & 16:
            return f'cop{cop} {w & 0x1ffffff:#x}'
        return f'.cop{cop} {w:#010x}'
    names = {32: 'lb', 33: 'lh', 34: 'lwl', 35: 'lw', 36: 'lbu', 37: 'lhu', 38: 'lwr',
             40: 'sb', 41: 'sh', 42: 'swl', 43: 'sw', 46: 'swr'}
    if op in names:
        return f'{names[op]} {R[rt]}, {simm}({R[rs]})'
    if op == 50:
        return f'lwc2 ${rt}, {simm}({R[rs]})'
    if op == 58:
        return f'swc2 ${rt}, {simm}({R[rs]})'
    return f'.word {w:#010x}'


def annotate(exe, start, count):
    """Disassemble with lui/addiu constant tracking for string/address annotation."""
    hi = {}
    out = []
    for i in range(count):
        pc = start + 4 * i
        if not exe.valid(pc):
            break
        w = exe.word(pc)
        txt = dis1(w, pc)
        op = w >> 26
        rs, rt = (w >> 21) & 31, (w >> 16) & 31
        note = ''
        v = None
        if op == 15:
            hi[rt] = (w & 0xffff) << 16
        elif op in (9, 13, 35, 43, 33, 37, 32, 36, 41, 40) and rs in hi:
            v = (hi[rs] + (sx16(w & 0xffff) if op != 13 else (w & 0xffff))) & 0xffffffff
            s = exe.cstr(v)
            note = f'  ; {v:#x}' + (f' "{s}"' if s else '')
        # register write tracking: addiu/ori produce a full constant, anything else clobbers
        dst = None
        if op == 0 and (w & 63) not in (8, 24, 25, 26, 27, 17, 19, 12, 13):
            dst = (w >> 11) & 31
        elif op in (8, 9, 10, 11, 12, 13, 14, 32, 33, 34, 35, 36, 37, 38):
            dst = rt
        elif op == 3:
            dst = 31
        if dst is not None:
            if op in (9, 13) and v is not None:
                hi[dst] = v
            else:
                hi.pop(dst, None)
        out.append(f'{pc:#010x}: {w:08x}  {txt}{note}')
    return out


def find_refs(exe, target):
    """Find instructions that materialize `target` via lui+addiu/ori/load/store, and jal to it."""
    res = []
    hi_t, lo_t = target >> 16, target & 0xffff
    n = len(exe.data) // 4
    for i in range(n):
        pc = exe.base + 4 * i
        w = exe.word(pc)
        op = w >> 26
        if op == 3 and ((pc & 0xf0000000) | ((w & 0x3ffffff) << 2)) == target:
            res.append((pc, 'call'))
        if op == 15:
            reg = (w >> 16) & 31
            imm = w & 0xffff
            for j in range(1, 24):
                pc2 = pc + 4 * j
                if not exe.valid(pc2):
                    break
                w2 = exe.word(pc2)
                op2 = w2 >> 26
                rs2 = (w2 >> 21) & 31
                if rs2 == reg and op2 in (9, 13, 35, 43, 33, 37, 32, 36, 41, 40, 34, 38):
                    v = ((imm << 16) + (sx16(w2 & 0xffff) if op2 != 13 else (w2 & 0xffff))) & 0xffffffff
                    if v == target:
                        res.append((pc2, 'ref'))
                # stop if reg overwritten
                rt2 = (w2 >> 16) & 31
                if op2 == 15 and rt2 == reg:
                    break
    return res


def func_start(exe, addr):
    """Heuristic: scan back for the `addiu sp, sp, -N` prologue."""
    a = addr
    while a > exe.base:
        w = exe.word(a)
        if (w >> 16) == 0x27bd and (w & 0x8000):
            return a
        a -= 4
    return addr


def func_end(exe, start):
    a = start
    while exe.valid(a):
        if exe.word(a) == 0x03e00008:  # jr ra
            # make sure it's the epilogue: next delay slot often addiu sp
            return a + 8
        a += 4
    return a


if __name__ == '__main__':
    cmd, path, addr = sys.argv[1], sys.argv[2], int(sys.argv[3], 16)
    exe = Exe(path)
    if cmd == 'dis':
        cnt = int(sys.argv[4]) if len(sys.argv) > 4 else 64
        print('\n'.join(annotate(exe, addr, cnt)))
    elif cmd == 'xref':
        for pc, kind in find_refs(exe, addr):
            print(f'{pc:#010x} {kind} in func {func_start(exe, pc):#010x}')
    elif cmd == 'func':
        s = func_start(exe, addr)
        e = func_end(exe, s)
        # functions may have multiple jr ra; extend while there is code until next prologue
        nxt = e
        while exe.valid(nxt) and not ((exe.word(nxt) >> 16) == 0x27bd and (exe.word(nxt) & 0x8000)):
            nxt += 4
        print('\n'.join(annotate(exe, s, (nxt - s) // 4)))
