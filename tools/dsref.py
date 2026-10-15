"""dsref.py OUTDIR [--state FILE|none] [--wait SECONDS] [--audio] [--keys K,K,...]  (K: key, hold:KEY:SECS, sleep:SECS,
shot:NAME (screenshot while running), down:KEY/up:KEY, ram:NAME (paused screenshot + RAM dump),
poke:ADDR:VALUE[:COUNT], save:NAME (save state into re/states/NAME.sav); --state NAME also
finds re/states/NAME.sav)

Capture a reference frame from DuckStation: start the emulator on a save state, let it run,
pause it over GDB, capture the window (PrintWindow) and dump main RAM. The Rust side
(`nfshs ref-compare OUTDIR`) renders the same view from the RAM dump for side-by-side
comparison. DuckStation is force-closed afterwards so it never writes a save state.

Needs [Debug] EnableGDBServer = true, GDBServerPort = 2345 in DuckStation's settings.ini.

--audio: DuckStation's sound is recorded too, into OUTDIR/audio.wav (44.1 kHz stereo, from
just before it starts): for the run its output goes, unmuted at full volume, to the silent
Steam Streaming Speakers device, whose loopback is recorded (settings.ini is restored after).
"""
import os
import struct, subprocess, sys, time
sys.path.insert(0, os.path.dirname(__file__))
import ds, dsshot

EXE = r'C:\Users\Branko\Desktop\Retro PS Games\PSX\duckstation-qt-x64-ReleaseLTCG.exe'
CUE = r'C:\Users\Branko\Desktop\Retro PS Games\PSX\ISOS\Need for Speed - High Stakes\Need for Speed - High Stakes (USA).cue'
STATE = os.path.expandvars(r'%USERPROFILE%\Documents\DuckStation\savestates\SLUS-00826_2.sav')
SETTINGS = os.path.expandvars(r'%USERPROFILE%\Documents\DuckStation\settings.ini')
# a playback device nobody listens to: DuckStation's sound goes there and is recorded back
AUDIO_DEVICE = '{0.0.0.00000000}.{5ec107bf-1a31-4431-824c-01a2c7174cc0}'


class Recorder:
    """Loopback recording of AUDIO_DEVICE on a thread until stop()."""

    def __init__(self):
        import threading
        self.chunks, self.done = [], threading.Event()
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def run(self):
        import soundcard as sc
        mic = sc.get_microphone(AUDIO_DEVICE, include_loopback=True)
        with mic.recorder(samplerate=44100, channels=2) as r:
            while not self.done.is_set():
                self.chunks.append(r.record(numframes=4410))

    def stop(self, path):
        import numpy as np, wave
        self.done.set()
        self.thread.join()
        data = np.concatenate(self.chunks) if self.chunks else np.zeros((0, 2))
        pcm = (np.clip(data, -1, 1) * 32767).astype('<i2')
        with wave.open(path, 'wb') as w:
            w.setnchannels(2)
            w.setsampwidth(2)
            w.setframerate(44100)
            w.writeframes(pcm.tobytes())
        print(f'audio: {path} ({len(pcm) / 44100:.1f} s)')


STATES = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 're', 'states')
SLOT_FILE = os.path.join(os.path.dirname(STATE), 'SLUS-00826_9.sav')
SLOT_SELECTED = False


def save_state(pid, name):
    if os.path.exists(SLOT_FILE):
        raise RuntimeError(f'{SLOT_FILE} exists: not overwriting slot 9 (the user may use it)')
    global SLOT_SELECTED
    for attempt in range(3):
        ds.focus_pid(pid)
        time.sleep(0.5)
        # the selected slot is 1 when DuckStation starts: F4 eight times selects slot 9
        if not SLOT_SELECTED:
            for _ in range(8):
                ds.tap('F4', hold=0.08, gap=0.15)
            SLOT_SELECTED = True
        ds.tap('F2', hold=0.1, gap=0.2)
        for _ in range(30):
            if os.path.exists(SLOT_FILE) and os.path.getsize(SLOT_FILE) > 100000:
                break
            time.sleep(0.2)
        if os.path.exists(SLOT_FILE):
            break
    time.sleep(1.0)
    if not os.path.exists(SLOT_FILE):
        raise RuntimeError(f'save state {name} was not written')
    os.makedirs(STATES, exist_ok=True)
    dst = os.path.join(STATES, name + '.sav')
    if os.path.exists(dst):
        os.remove(dst)
    os.replace(SLOT_FILE, dst)
    backup = SLOT_FILE + '.backup'
    if os.path.exists(backup):
        os.remove(backup)
    print(f'saved state {dst}')


def arg(name, default):
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else default


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    state = arg('--state', STATE)
    if state != 'none' and not os.path.exists(state) and os.path.exists(os.path.join(STATES, state + '.sav')):
        state = os.path.join(STATES, state + '.sav')
    wait = float(arg('--wait', '3'))
    keys = [k for k in arg('--keys', '').split(',') if k]
    audio = '--audio' in sys.argv
    settings = open(SETTINGS, encoding='utf-8').read() if audio else None
    recorder = None
    if audio:
        import re
        s2 = re.sub(r'(?m)^OutputMuted = .*$', 'OutputMuted = false', settings)
        s2 = re.sub(r'(?m)^OutputVolume = .*$', 'OutputVolume = 100', s2)
        s2 = re.sub(r'(?m)^OutputDevice = .*$', 'OutputDevice = ' + AUDIO_DEVICE, s2)
        open(SETTINGS, 'w', encoding='utf-8').write(s2)
        recorder = Recorder()
    # --state none: boot the disc from the start (front end)
    proc = subprocess.Popen([EXE, '-nofullscreen'] + ([] if state == 'none' else ['-statefile', state]) + [CUE])
    try:
        time.sleep(wait)
        # DuckStation may show its update notice, which takes the keyboard: close it (= remind later)
        for h, title in ds.windows_of(proc.pid):
            if 'updater' in title.lower():
                ds.user32.PostMessageW(h, 0x0010, 0, 0)
                time.sleep(0.5)
        # attach once (this halts the CPU), let it run while driving, then interrupt
        g = ds.Gdb(timeout=30)
        g.cmd('?')
        g.cont()
        if keys:
            ds.focus_pid(proc.pid)
            for k in keys:
                if k.startswith('sleep:'):
                    time.sleep(float(k.split(':')[1]))
                elif k.startswith('shot:'):
                    # a screenshot while running (OUTDIR/NAME.png)
                    game = [w for w in ds.windows_of(proc.pid) if 'need for speed' in w[1].lower()]
                    dsshot.capture_to(game[0][0], os.path.join(out, k.split(':')[1] + '.png'))
                elif k.startswith('ram:'):
                    # paused: a screenshot and the RAM (OUTDIR/NAME.png, NAME.bin), then on
                    g.interrupt()
                    time.sleep(0.3)
                    game = [w for w in ds.windows_of(proc.pid) if 'need for speed' in w[1].lower()]
                    name = k.split(':')[1]
                    dsshot.capture_to(game[0][0], os.path.join(out, name + '.png'))
                    open(os.path.join(out, name + '.bin'), 'wb').write(g.read(0x80000000, 0x200000))
                    g.cont()
                    ds.focus_pid(proc.pid)
                elif k.startswith('save:'):
                    # save:NAME: a DuckStation save state of this moment as re/states/NAME.sav
                    # (made in slot 9, which is then cleared, so the user's slots stay as
                    # they are)
                    save_state(proc.pid, k.split(':', 1)[1])
                elif k.startswith('poke:'):
                    # poke:ADDR:VALUE[:COUNT]: 32-bit words into RAM (hex address)
                    parts = k.split(':')
                    addr, val = int(parts[1], 16), int(parts[2], 0)
                    n = int(parts[3]) if len(parts) > 3 else 1
                    g.interrupt()
                    time.sleep(0.2)
                    g.write(addr, struct.pack('<I', val) * n)
                    g.cont()
                    ds.focus_pid(proc.pid)
                elif k.startswith('down:'):
                    # down:KEY / up:KEY: press and release separately (shots while held)
                    ds.down(k.split(':')[1])
                elif k.startswith('up:'):
                    ds.up(k.split(':')[1])
                elif k.startswith('hold:'):
                    _, key, secs = k.split(':')
                    ds.down(key)
                    time.sleep(float(secs))
                    ds.up(key)
                else:
                    ds.tap(k)
        g.interrupt()
        time.sleep(0.3)
        game = [w for w in ds.windows_of(proc.pid) if 'need for speed' in w[1].lower()]
        dsshot.capture_to(game[0][0], os.path.join(out, 'window.png'))
        ram = g.read(0x80000000, 0x200000)
        open(os.path.join(out, 'ram.bin'), 'wb').write(ram)
        print(f'captured {out}: window.png, ram.bin ({len(ram)} bytes)')
    finally:
        proc.kill()
        proc.wait()
        if settings is not None:
            open(SETTINGS, 'w', encoding='utf-8').write(settings)
            recorder.stop(os.path.join(out, 'audio.wav'))


if __name__ == '__main__':
    main()
