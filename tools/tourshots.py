"""Run every tour in docs/DECOMP.md with the decompiled code on and off, screenshot both every
STEP frames, and report the frames that differ (and the memory cards the tours saved to).

`nfshs decomp-check` compares RAM, so it can't see what only the host does with the game's
calls: the showroom car it draws, primitives it captures, its autosave. A port that calls a
hooked function the wrong way (see hookaudit.py) changes only the picture; this catches it.

    python tools/tourshots.py [--step N] [--tour K ...] [--out DIR]

Build first (`cargo build --release`). Each tour runs on its own copy of memcard.mcd. The
screenshots go to OUT/t<K>_on and OUT/t<K>_off (default target/tourshots), fe_<frame>.png;
compare a differing pair by eye. Exit status 1 when any frame differs.
"""
import argparse
import concurrent.futures as cf
import hashlib
import os
import re
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def tours():
    doc = open(os.path.join(ROOT, 'docs/DECOMP.md'), encoding='utf8').read()
    return re.findall(r'`BOOT_SCRIPT="([^"]+)"', doc)


def frames(script):
    return sum(round(float(s.split(':')[0]) * 60) for s in script.split(','))


def md5(p):
    with open(p, 'rb') as f:
        return hashlib.md5(f.read()).hexdigest()


def run(exe, script, out, mode, step):
    shutil.rmtree(out, ignore_errors=True)
    os.makedirs(out)
    card = os.path.join(out, 'card.mcd')
    shutil.copy(os.path.join(ROOT, 'memcard.mcd'), card)
    shots = ','.join(str(f) for f in range(step, frames(script), step))
    env = dict(os.environ, BOOT_SCRIPT=script, NFSHS_MEMCARD=card, FE_SHOT=f'{shots}:{out}', NFSHS_DECOMP=mode)
    for k in ('NFSHS_DECOMP_OFF', 'NFSHS_DECOMP_ONLY', 'NFSHS_DECOMP_CHECK'):
        env.pop(k, None)
    done = subprocess.run([exe, 'boot-main'], cwd=ROOT, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=1800)
    return done.returncode, done.stderr.decode(errors='replace')[-500:]


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--step', type=int, default=10, help='frames between screenshots (default 10)')
    ap.add_argument('--tour', type=int, action='append', help='only these tours (0 = the first in DECOMP.md)')
    ap.add_argument('--out', default=os.path.join(ROOT, 'target', 'tourshots'))
    ap.add_argument('--jobs', type=int, default=os.cpu_count() or 4)
    a = ap.parse_args()

    exe = os.path.join(ROOT, 'target', 'release', 'nfshs.exe' if os.name == 'nt' else 'nfshs')
    if not os.path.exists(exe):
        sys.exit(f'{exe} not found: cargo build --release first')
    scripts = tours()
    which = a.tour if a.tour else range(len(scripts))
    with cf.ThreadPoolExecutor(a.jobs) as ex:
        jobs = {(k, m): ex.submit(run, exe, scripts[k], os.path.join(a.out, f't{k}_{m}'), m, a.step)
                for k in which for m in ('on', 'off')}
        bad = 0
        for k in which:
            on, off = os.path.join(a.out, f't{k}_on'), os.path.join(a.out, f't{k}_off')
            for m in ('on', 'off'):
                code, err = jobs[(k, m)].result()
                if code != 0:
                    print(f'tour {k}: the {m} run exited with {code:#x}\n{err}')
                    bad += 1
            shots = sorted({n for n in os.listdir(on) + os.listdir(off) if n.endswith('.png')}, key=lambda n: int(re.sub(r'\D', '', n)))
            diff = [n for n in shots if not (os.path.exists(os.path.join(on, n)) and os.path.exists(os.path.join(off, n)))
                    or md5(os.path.join(on, n)) != md5(os.path.join(off, n))]
            card = 'same' if md5(os.path.join(on, 'card.mcd')) == md5(os.path.join(off, 'card.mcd')) else 'DIFFERENT'
            frames_ = ', '.join(re.sub(r'\D', '', n) for n in diff[:10]) + (' ...' if len(diff) > 10 else '')
            print(f'tour {k} ({frames(scripts[k])} frames, {len(shots)} shots): '
                  + (f'{len(diff)} frames differ: {frames_}' if diff else 'all same') + f'; memory card {card}', flush=True)
            bad += bool(diff) or card != 'same'
    if bad:
        print(f'screenshots in {a.out}: t<K>_on/fe_<frame>.png against t<K>_off/')
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
