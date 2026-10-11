"""export_snap.py snap.pkl OUTDIR: pickle snapshot -> raw files for the Rust tools."""
import pickle, struct, os, sys
s = pickle.load(open(sys.argv[1], 'rb'))
d = sys.argv[2]; os.makedirs(d, exist_ok=True)
for k in ('before', 'after'):
    if k not in s: continue
    open(f'{d}/{k}_ram.bin', 'wb').write(s[k]['ram'])
    open(f'{d}/{k}_scratch.bin', 'wb').write(s[k]['scratch'])
    open(f'{d}/{k}_regs.bin', 'wb').write(struct.pack('<%dI' % len(s[k]['regs']), *s[k]['regs']))
open(f'{d}/func.txt', 'w').write(hex(s['func']))
print('exported', d)
