"""snap.py FUNC OUTPREFIX [keys]: freeze at FUNC entry (a0 == player car), dump RAM+scratch+regs,
run to return, dump RAM+scratch+regs again."""
import ds, struct, sys, time, pickle
CAR = 0x801C19A8
f = int(sys.argv[1], 16); out = sys.argv[2]; keys = sys.argv[3:]
g = ds.Gdb(timeout=30); g.cmd('?')
ds.focus()
if keys: ds.down(*keys)
g.cont(); time.sleep(1.0); g.interrupt()
g.cmd(f'Z0,{f:x},4')
for _ in range(60):
    g.cont(); g.wait_stop(timeout=15)
    r = g.regs()
    if r[37] == f and r[4] == CAR: break
g.cmd(f'z0,{f:x},4')
if keys: ds.up(*keys)
t = time.time()
before = {'regs': r, 'ram': g.read(0x80000000, 0x200000), 'scratch': g.read(0x1F800000, 0x400)}
print('before dumped in', round(time.time() - t), 's')
ra = r[31]
g.cmd(f'Z0,{ra:x},4')
g.cont(); g.wait_stop(timeout=15)
g.cmd(f'z0,{ra:x},4')
r2 = g.regs()
after = {'regs': r2, 'ram': g.read(0x80000000, 0x200000), 'scratch': g.read(0x1F800000, 0x400)}
g.cont(); time.sleep(0.2); g.s.close()
pickle.dump({'func': f, 'before': before, 'after': after}, open(out, 'wb'))
print('saved', out, 'pc after', hex(r2[37]))
