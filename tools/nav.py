"""nav.py KEY[:hold] ... then screenshot to $OUT (default scratch ds.png). KEY may be 'wait2' to sleep."""
import ds, dsshot, sys, time, os
OUT = os.environ.get('OUT', 'C:/Users/Branko/AppData/Local/Temp/claude/C--Users-Branko/ab1a0b9b-f54f-49aa-86d3-9d18688901d6/scratchpad/ds.png')
ds.focus()
for a in sys.argv[1:]:
    if a.startswith('wait'):
        time.sleep(float(a[4:]))
        continue
    k, _, hold = a.partition(':')
    ds.tap(k, hold=float(hold) if hold else 0.1, gap=0.35)
time.sleep(0.5)
print(dsshot.shot(OUT))
