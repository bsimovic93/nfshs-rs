"""fdis.py ADDR [--calls]: disassemble a whole function (recompiler boundaries)."""
import sys; sys.path.insert(0, '.')
import mips
from recomp import func_end
e = mips.Exe('../gamedata/NFS4.EXE')
a = int(sys.argv[1], 16)
lines = mips.annotate(e, a, (func_end(a) - a) // 4)
if '--calls' in sys.argv:
    lines = [l for l in lines if ' jal' in l or '"' in l or 'jr ' in l or 'syscall' in l]
print('\n'.join(lines))
