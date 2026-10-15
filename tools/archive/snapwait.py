"""snapwait.py FUNC OUT: wait (forever) for FUNC to execute, then snapshot RAM/scratch/regs (no 'after')."""
import ds, sys, time, pickle
f = int(sys.argv[1], 16); out = sys.argv[2]
g = ds.Gdb(timeout=30); g.cmd('?')
g.cmd(f'Z0,{f:x},4')
g.cont()
while True:
    try:
        g.wait_stop(timeout=3600); break
    except TimeoutError:
        continue
r = g.regs()
g.cmd(f'z0,{f:x},4')
snap = {'func': f, 'before': {'regs': r, 'ram': g.read(0x80000000, 0x200000), 'scratch': g.read(0x1F800000, 0x400)}}
pickle.dump(snap, open(out, 'wb'))
print('hit', hex(r[37]), 'ra', hex(r[31]), 'a0', hex(r[4]), 'a1', hex(r[5]))
# keep CPU halted so the next tool can attach; leave it stopped
g.s.close()
