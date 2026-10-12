"""dsref.py OUTDIR [--state FILE] [--wait SECONDS] [--keys K,K,...]

Capture a reference frame from DuckStation: start the emulator on a save state, let it run,
pause it over GDB, capture the window (PrintWindow) and dump main RAM. The Rust side
(`nfshs ref-compare OUTDIR`) renders the same view from the RAM dump for side-by-side
comparison. DuckStation is force-closed afterwards so it never writes a save state.

Needs [Debug] EnableGDBServer = true, GDBServerPort = 2345 in DuckStation's settings.ini.
"""
import os, subprocess, sys, time
sys.path.insert(0, os.path.dirname(__file__))
import ds, dsshot

EXE = r'C:\Users\Branko\Desktop\Retro PS Games\PSX\duckstation-qt-x64-ReleaseLTCG.exe'
CUE = r'C:\Users\Branko\Desktop\Retro PS Games\PSX\ISOS\Need for Speed - High Stakes\Need for Speed - High Stakes (USA).cue'
STATE = os.path.expandvars(r'%USERPROFILE%\Documents\DuckStation\savestates\SLUS-00826_2.sav')


def arg(name, default):
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else default


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    state = arg('--state', STATE)
    wait = float(arg('--wait', '3'))
    keys = [k for k in arg('--keys', '').split(',') if k]
    proc = subprocess.Popen([EXE, '-nofullscreen', '-statefile', state, CUE])
    try:
        time.sleep(wait)
        if keys:
            ds.focus('Need for Speed')
            for k in keys:
                if k.startswith('hold:'):
                    _, key, secs = k.split(':')
                    ds.down(key)
                    time.sleep(float(secs))
                    ds.up(key)
                else:
                    ds.tap(k)
        g = ds.Gdb(timeout=30)
        g.cmd('?')  # attaching halts the emulated CPU
        time.sleep(0.3)
        dsshot.shot(os.path.join(out, 'window.png'))
        ram = g.read(0x80000000, 0x200000)
        open(os.path.join(out, 'ram.bin'), 'wb').write(ram)
        print(f'captured {out}: window.png, ram.bin ({len(ram)} bytes)')
    finally:
        proc.kill()
        proc.wait()


if __name__ == '__main__':
    main()
