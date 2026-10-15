"""explore.py snap.pkl [max_steps]: run a snapshotted function to return, trapping hardware & BIOS."""
import snapemu, pickle, sys, collections, hle
hle.install()
snap = pickle.load(open(sys.argv[1], 'rb'))
b = snap['before']
mem = snapemu.Mem(b['ram'], b['scratch'])
mem.io_log = []
mem.track = False
try:
    regs, steps = snapemu.run(mem, b['regs'], snap['func'], b['regs'][31], max_steps=int(sys.argv[2]) if len(sys.argv) > 2 else 50_000_000)
    print('returned after', steps, 'steps, v0 =', hex(regs[2]))
except Exception as ex:
    print('stopped:', ex)
by_func = collections.Counter((k, f) for k, a, v, f in mem.io_log)
print('hardware/BIOS access by function:')
for (k, f), n in sorted(by_func.items(), key=lambda x: -x[1])[:40]:
    print(f'  {k:5} in {f:#x}: {n}')
bios = collections.Counter((a, v) for k, a, v, f in mem.io_log if k == 'bios')
print('BIOS calls (vector, t1):', [(hex(a), hex(v), n) for (a, v), n in bios.items()])
print('files:', hle.file_log[:40])

