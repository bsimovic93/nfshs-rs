"""watch.py ADDR [keys...]: write-watch ADDR while holding keys; print the PC/ra of the first few hits."""
import ds, sys, time
addr = int(sys.argv[1], 16)
keys = sys.argv[2:]
N = ['zero','at','v0','v1','a0','a1','a2','a3','t0','t1','t2','t3','t4','t5','t6','t7','s0','s1','s2','s3','s4','s5','s6','s7','t8','t9','k0','k1','gp','sp','fp','ra']
g = ds.Gdb(timeout=10); g.cmd('?')
g.cmd(f'Z2,{addr:x},4')
ds.focus()
if keys: ds.down(*keys)
seen = []
try:
    for _ in range(int(__import__('os').environ.get('HITS', '3'))):
        g.cont()
        g.wait_stop(timeout=10)
        r = g.regs()
        print('pc', hex(r[37]), 'ra', hex(r[31]), ' '.join(f'{N[i]}={r[i]:#x}' for i in (2,3,4,5,6,7,16,17,18,19,20,21,22,23)))
finally:
    if keys: ds.up(*keys)
    g.cmd(f'z2,{addr:x},4'); g.cont(); time.sleep(0.2); g.s.close()
