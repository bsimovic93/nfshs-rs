"""ramdump.py OUT: dump 2 MB PS1 main RAM via GDB (briefly halts the CPU)."""
import ds, sys
g = ds.Gdb(timeout=20)
data = g.read(0x80000000, 0x200000)
g.cont(); g.s.close()
open(sys.argv[1], 'wb').write(data)
print(sys.argv[1], len(data))
