"""Break on a set of init functions; on each first hit snapshot RAM and log order/args."""
import ds, sys, time, pickle
funcs = [int(x, 16) for x in sys.argv[1:]]
g = ds.Gdb(timeout=30); g.cmd('?')
for f in funcs:
    g.cmd(f'Z0,{f:x},4')
g.cont()
remaining = set(funcs)
log = []
while remaining:
    try:
        g.wait_stop(timeout=3600)
    except Exception:
        continue
    r = g.regs(); pc = r[37]
    if pc in remaining:
        remaining.discard(pc)
        g.cmd(f'z0,{pc:x},4')
        snap = {'func': pc, 'before': {'regs': r, 'ram': g.read(0x80000000, 0x200000), 'scratch': g.read(0x1F800000, 0x400)}}
        pickle.dump(snap, open(f'../re/ram/init_{pc:08x}.pkl', 'wb'))
        log.append((pc, r[31], r[4], r[5], r[6]))
        print(f'{len(log)}: {pc:#x} ra={r[31]:#x} a0={r[4]:#x} a1={r[5]:#x} a2={r[6]:#x}', flush=True)
    g.cont()
g.s.close()
