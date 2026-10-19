"""fdis.py ADDR [--calls]: disassemble a whole function (recompiler boundaries), with the names
of the functions it calls and the globals it reads and writes (re/retail_syms.tsv,
re/retail_globals.tsv) where they are known."""
import bisect
import re
import sys; sys.path.insert(0, '.')
import mips
from recomp import func_end

GP = 0x8013D83C
e = mips.Exe('../gamedata/NFS4.EXE')
a = int(sys.argv[1], 16)

funcs = {}
glob = {}
try:
    for line in open('../re/retail_syms.tsv'):
        r, n, _ = line.rstrip('\n').split('\t')
        funcs[int(r, 16)] = n.split('__')[0]
    for line in open('../re/retail_globals.tsv', encoding='utf-8'):
        f = line.rstrip('\n').split('\t')
        glob[int(f[0], 16)] = f[1]
except FileNotFoundError:
    pass
gstarts = sorted(glob)


def gname(v):
    i = bisect.bisect_right(gstarts, v) - 1
    if i < 0 or v - gstarts[i] >= 0x400:
        return None
    off = v - gstarts[i]
    return glob[gstarts[i]] + (f'+{off:#x}' if off else '')


lines = mips.annotate(e, a, (func_end(a) - a) // 4)
out = []
for l in lines:
    m = re.search(r'jal (0x[0-9a-f]+)', l)
    if m and int(m[1], 16) in funcs:
        l += f'  ; {funcs[int(m[1], 16)]}'
    m = re.search(r'(-?\d+)\(gp\)', l)
    if m:
        n = gname((GP + int(m[1])) & 0xFFFFFFFF)
        if n:
            l += f'  ; {n}'
    m = re.search(r'; (0x8[0-9a-f]{7})', l)
    if m:
        n = gname(int(m[1], 16))
        if n:
            l += f' {n}'
    out.append(l)
if '--calls' in sys.argv:
    out = [l for l in out if ' jal' in l or '"' in l or 'jr ' in l or 'syscall' in l]
print('\n'.join(out))
