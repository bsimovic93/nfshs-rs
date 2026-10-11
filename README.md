# nfshs-rs — Need for Speed: High Stakes (PS1) rebuilt for PC in Rust

A from-scratch engine that runs the original PlayStation release of *Need for Speed: High Stakes*
(NTSC-U, SLUS-00826) natively on PC. It loads assets straight from your own disc image: no
emulation, and no game data in this repository.

All decoders come from reverse engineering the disc and `NFS4.EXE`. See
[`docs/FORMATS.md`](docs/FORMATS.md) for the format specification.

## What works

- **Disc**: reads the raw `.bin` (MODE2/2352, ISO9660) and extracts it.
- **Assets**: VIV archives, PSH textures, car models, track geometry, props and tree
  sprites, sky panoramas, and all three EA compression codecs. The codecs are ported from
  the game's own MIPS routines and verified byte-for-byte against emulated originals.
- **Tracks**: all 10 tracks on the disc, in day / night / wet / snow variants.
- **Cars**: 16 player cars, with performance taken from each car's `.QDA` engine data
  (redline, gear speeds, acceleration curve, top speed).
- **Racing**: AI opponents, a starting grid, countdown, laps, positions, car-to-car
  collisions, the original track boundaries, and a results screen.
- **Audio**: engine sound layers from the car's sound bank, plus the original in-race and
  menu music (PS1 ADPCM).
- **Front end**: built from the original title, track and car artwork.
- **Rendering**: wgpu, PS1-style (nearest filtering, texture × vertex colour, fog), with
  night headlights.
- **Input**: keyboard and gamepad.

## Build & run

Requires Rust (stable) and your own copy of the game disc image.

```sh
cargo build --release
# extract the disc (once)
./target/release/nfshs extract "Need for Speed - High Stakes (USA).bin" gamedata
# play (opens the menu; gamedata/ is found next to the project, or pass --data DIR / set NFSHS_DATA)
./target/release/nfshs --data gamedata
# or jump straight into a race
./target/release/nfshs play --track 5 --variant N --car F375 --opponents MCF1,DIAB --laps 3
```

Tracks: `0` Snowy Ridge, `2` Dolphin Cove, `3` Route Adonf, `4` Kindiak Park, `5` Celtic Ruins,
`6` Landstrasse, `7` Durham Road, `8` Raceway 2, `9` Raceway, `10` Raceway 3.
Conditions (`--variant`): none = day, `N` night, `W` wet, `S` snow.

### Controls

| Action | Keyboard | Gamepad |
|---|---|---|
| Throttle / brake–reverse | Up / Down (W / S) | RT / LT (or A / X) |
| Steer | Left / Right (A / D) | Left stick |
| Handbrake | Space | B |
| Camera | C | Y |
| Reset to road | R | Back |
| Autopilot (test) | P | |
| Menu / back | Esc | |

## Developer commands

```
nfshs dump-psh <file.psh|viv> out.png        texture sheet
nfshs dump-car ZZZBMWM.VIV out.png [parts]   software-rendered car preview
nfshs dump-track ZTR00.GRP ZTR000.PSH out.png [node]
nfshs dump-audio <bank.viv|music.viv> outdir decode to WAV
nfshs verify-decompress <files...>           compare codecs against re/dec/*.bin
nfshs shot --track N --time S --keys u|a     headless simulation + screenshot
nfshs probe --from A --to B                  collision probe along the virtual road
```

`tools/` holds the Python RE helpers used during analysis: a MIPS disassembler with xref
search (`mips.py`), a MIPS interpreter that runs the game's own routines as ground truth
(`mipsemu.py`), a PS-X EXE to ELF wrapper, and GRP chunk dumpers.

## Not done yet

Hot Pursuit (police AI, sirens, roadblocks), traffic, High Stakes / tournament career modes,
damage, split-screen, FMV playback, the exact engine-sound mixing tables (`CRDl`), weather
particles, and replays.
