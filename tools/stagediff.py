"""stagediff.py FUNC... [-k KEY]: snapshot the player car struct around one call of each FUNC."""
import ds, struct, time, sys
CAR = 0x801C19A8
SIZE = 0x8F0
args = sys.argv[1:]
keys = []
if '-k' in args:
    i = args.index('-k'); keys = args[i + 1:]; args = args[:i]
funcs = [int(a, 16) for a in args]
g = ds.Gdb(timeout=15); g.cmd('?')
ds.focus()
if keys: ds.down(*keys)
g.cont(); time.sleep(0.8); g.interrupt()
res = {}
try:
    for f in funcs:
        g.cmd(f'Z0,{f:x},4')
        for _ in range(40):
            g.cont(); g.wait_stop(timeout=15)
            r = g.regs()
            if r[37] == f and r[4] == CAR:
                break
        g.cmd(f'z0,{f:x},4')
        before = g.read(CAR, SIZE)
        ra = r[31]
        g.cmd(f'Z0,{ra:x},4')
        for _ in range(40):
            g.cont(); g.wait_stop(timeout=15)
            if g.regs()[37] == ra: break
        g.cmd(f'z0,{ra:x},4')
        res[f] = (before, g.read(CAR, SIZE))
finally:
    if keys: ds.up(*keys)
    g.cont(); time.sleep(0.2); g.s.close()
for f, (b, a) in res.items():
    wb = struct.unpack_from('<%di' % (SIZE // 4), b); wa = struct.unpack_from('<%di' % (SIZE // 4), a)
    ch = [(i * 4, wb[i], wa[i]) for i in range(len(wb)) if wb[i] != wa[i]]
    print(f'== {f:#x}: {len(ch)} words changed')
    print('   ' + ' '.join(f'+{o:#x}:{x}->{y}' for o, x, y in ch[:80]))
    open(f'../re/ram/stage_{f:x}.bin', 'wb').write(b + a)
