"""sample.py OUT.npz-like: repeatedly halt, read a RAM window, resume; while pressing scripted keys.
usage: sample.py out.bin start_hex length_hex n_samples interval_s [keys_down...]"""
import ds, sys, time, struct
out, start, length, n, dt = sys.argv[1], int(sys.argv[2], 16), int(sys.argv[3], 16), int(sys.argv[4]), float(sys.argv[5])
keys = sys.argv[6:]
g = ds.Gdb(timeout=10)
g.cmd('?')
frames = []
ds.focus()
g.cont()
if keys:
    ds.down(*keys)
try:
    for i in range(n):
        time.sleep(dt)
        g.interrupt()
        t = time.time()
        frames.append(g.read(start, length))
        g.cont()
finally:
    if keys:
        ds.up(*keys)
time.sleep(0.2)
g.interrupt(); g.cont(); time.sleep(0.2); g.s.close()
with open(out, 'wb') as f:
    f.write(struct.pack('<III', start, length, len(frames)))
    for fr in frames:
        f.write(fr)
print('saved', len(frames), 'frames')
