"""appdrive.py EXE OUTDIR KEYS: run the nfshs window, drive it with keys and capture it.

KEYS: comma-separated steps: a key name (tap), hold:KEY:SECS, sleep:SECS, shot:NAME.
Key names: ds.VK's plus UP DOWN LEFT RIGHT SPACE BACK.
"""
import os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(__file__))
import ds, dsshot

ds.VK.update({'UP': 0x26, 'DOWN': 0x28, 'LEFT': 0x25, 'RIGHT': 0x27, 'SPACE': 0x20, 'BACK': 0x08})
EXTENDED = {0x25, 0x26, 0x27, 0x28}
_plain_key = ds._key


def _key(vk, up):
    # arrow keys are extended keys: without the flag they arrive as the numpad's
    if vk not in EXTENDED:
        return _plain_key(vk, up)
    i = ds.INPUT(type=1)
    i.ki = ds.KEYBDINPUT(vk, ds.user32.MapVirtualKeyW(vk, 0), 0x1 | (0x2 if up else 0), 0, 0)
    ds.user32.SendInput(1, ds.ctypes.byref(i), ds.ctypes.sizeof(ds.INPUT))


ds._key = _key


def main():
    exe, out, keys = sys.argv[1], sys.argv[2], [k for k in sys.argv[3].split(',') if k]
    os.makedirs(out, exist_ok=True)
    proc = subprocess.Popen([exe], cwd=os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    try:
        time.sleep(4)
        win = lambda: [w for w in ds.windows_of(proc.pid)]
        ds.focus_pid(proc.pid)
        for k in keys:
            if k.startswith('sleep:'):
                time.sleep(float(k.split(':')[1]))
            elif k.startswith('hold:'):
                _, key, secs = k.split(':')
                ds.focus_pid(proc.pid)
                ds.down(key)
                time.sleep(float(secs))
                ds.up(key)
            elif k.startswith('shot:'):
                dsshot.capture_to(win()[0][0], os.path.join(out, k.split(':')[1] + '.png'))
            else:
                ds.focus_pid(proc.pid)
                ds.tap(k, hold=0.12)
        print('done')
    finally:
        proc.kill()
        proc.wait()


if __name__ == '__main__':
    main()
