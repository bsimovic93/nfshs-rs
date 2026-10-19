# Debugging switches

Environment variables the code reads for diagnostics. A `=1` variable only needs to be set;
the others take the value shown. Player-facing settings are in the README.

## Headless runs (`nfshs shot`, `fe-play`, `boot-race`)

| Variable | Effect |
|---|---|
| `SHOT_SIZE=WxH` | `shot`, `fe-play`: the pictures' size (default 960x720), e.g. `1920x1080`. |
| `SHOT_DT=SECS` | `shot`: the render step (a display faster than 60 Hz). |
| `SHOT_CAMLOG=1` | `shot`: each frame's camera and player car, to look for jitter. |
| `NFSHS_OBJLOG=1` | `shot`: the track objects the original drew in the last frame (model, world position, matrix). |
| `SHOT_SCRIPT="t:keys,..."` | `shot`: from second t on, those keys (as `--keys`: u d l r h a). |
| `SHOT_TRACE=1` | `shot`: how far the player's car moved in each of the last frames. |
| `SHOT_SERIES=SECS` | `shot`: also a picture every SECS seconds (`OUT_<t>.png`) and where the car was, to find a place. |
| `NFSHS_CAM="x,y,z,tx,ty,tz"` | `shot`: take the picture from a fixed point (several separated by `;`: `OUT_cam<i>.png` each). |
| `NFSHS_VRAMDUMP=FILE` | `shot`: the original's VRAM (1024×512, 16-bit) at the end. |
| `NFSHS_GLOWLOG=1` | `shot`: the light glows the original drew in the last frame (world anchors). |
| `NFSHS_SKYLOG=1` | `shot`: the sky dome's colours and the depth cue every 2 s. |
| `NFSHS_SCROLLLOG=1` | `shot`: the scrolling track textures' offsets (material, rows) every 10 frames. |
| `NFSHS_LIGHTTABLELOG=1` | `shot`: a checksum of the original's light table every 5 s. |
| `NFSHS_FLIPLOG=1` | `shot`, `fe-play`: when the car tips over. |
| `FEPLAY_PAD=dpad\|stick` | `fe-play`: a gamepad (directions from the d-pad or the stick). |
| `NFSHS_CARSLOG=1` | `fe-play`: every second, the cars' distances and each view's camera. |
| `NFSHS_PARTSLOG=N` | `fe-play`: every N frames, the parts the original draws of each car. |
| `NFSHS_BEAMLOG=1` | `fe-play`: every second, the night headlight beam's parameters. |
| `NFSHS_COLOUR=N` | The player's paint (0..15) in races started without the front end. |
| `NFSHS_TRAFFIC=N` | Traffic at density N in races started without the front end. |
| `BOOT_SCRIPT="secs:buttons,..."` | `boot-race`, `boot-main`: pad input per stretch of time. |
| `GS_SET="hexoff=value,..."` | `boot-race`, `shot`: poke words of the GameSetup (`3d4=27`: the player in the police Diablo; the next car's type is at `488`). |
| `BOOT_TRACE=1` | `boot-race`: per frame, how far each car moved. |
| `BOOT_RAM=1` | `boot-race`: write RAM to `re/boot` after booting. |
| `BOOT_FRAMES=1`, `BOOT_DUMP_FRAME=N`, `BOOT_RAM_FRAME=N` | `boot-main`: per-frame summary, frame N's 2D, RAM at frame N. |
| `PS1_TRACE=1`, `TICKS=N` | `boot-main`: trace the CPU; stop after N ticks. |

## What the original draws

| Variable | Effect |
|---|---|
| `NFSHS_HUDLOG=1\|2` | The commands recorded from each HUD view (2: each command). |
| `NFSHS_HUD_DEBUG=1` | Texels the HUD port cannot find. |
| `NFSHS_SUNLOG=1` | The sun's primitives, its lens flare's pieces (and how far each follows the sun), and the visibility test: where the sun is tested, what the renderer saw, what the game read back. |
| `NFSHS_SMOKELOG=1`, `NFSHS_WEATHERLOG=1` | The smoke puffs and the weather's primitives. |
| `NFSHS_NOSMOKE=1` | Smoke puffs not drawn. |
| `NFSHS_SKIDLOG=1` | Tyre marks: how many, how many near, which slices. |
| `NFSHS_TRAFLOG=1` | Nearby traffic cars' positions. |
| `NFSHS_STATIC_OBJECTS=1` | The track objects from the track's data, not as the original drew them this frame. |
| `NFSHS_HIDE_PART=a,b,...` | Car parts (GEO part numbers) not drawn. |
| `NFSHS_ENVLOG=1` | Every 2 s, each view's GTE environment (far colour, depth cue). |
| `NFSHS_ENVDBG=1` | Each DrawOTag call. |
| `PS1_CMDLOG=1`, `PS1_OTLOG=1`, `PS1_OTLOG_PRIM=1` | The front end's recorded commands and their ordering-table entries. |
| `PS1_MISSLOG=1` | Front-end packets the original built that were not recorded. |
| `PS1_PRIMLOG=1` | GPU primitives the recorder does not draw. |
| `PS1_OVLOG=1` | The HUD's filled boxes. |
| `PS1_VIDLOG=1` | The track select's video quads. |

## Assets

| Variable | Effect |
|---|---|
| `NFSHS_PAINT=N` | The player's car's textures in paint N (colour 0..15, 8..15 the "d" sheet; +16, +32: the palette flags). |
| `NFSHS_CAR_PAGES=DIR` | Save each car's assembled texture pages. |
| `NFSHS_DUMP_MODEL=N` | Track prop model N's quads. |
| `NFSHS_DUMP_MATERIALS=1` | The scrolling materials (flag 0x80): shape, UV flags, interval, where the first quad is. |
| `NFSHS_DUMP_PROPS=1` | How often each prop model is placed. |
| `NFSHS_DUMP_SHADES=1` | The range of the track's vertex shades. |
| `NFSHS_STRIP_ORIENT=o` | Strip quads' corners turned by o%4 (mirrored if o≥4). |
| `NFSHS_NO_HORIZON=1` | No horizon band. |
| `NFSHS_LIGHTLOG=1` | The original's light table at race start. |
| `NFSHS_BLOCKS=1` | `dump-track`: the first blocks' sizes. |
| `EXTRACT_ONLY=suffix` | `extract`: only the files ending in it (e.g. `.XA`). |

## Reference comparisons (`nfshs ref-compare`)

| Variable | Effect |
|---|---|
| `NFSHS_REF_CAR=CODE` | Another car's model at the player's pose. |
| `NFSHS_REF_NOCAR=1` | No player car (to see what is under it). |
| `NFSHS_REF_OVERLAY=pause` | The pause menu as the game draws it. |
| `NFSHS_PICK="x0,y0,x1,y1"` | The track quads whose centre falls in that screen box (also `shot`, 960x720). |

## The machine

| Variable | Effect |
|---|---|
| `PS1_CALLLOG=addr,...`, `PS1_CALLSTACK=1` | Log those host calls (with the call stack). |
| `PS1_FILELOG=1`, `PS1_FILES=1` | The files the game opens; every file looked up. |
| `NFSHS_MCLOG=1` | The memory card calls and their results. |
| `NFSHS_GSLOG=1` | The GameSetup a race starts with. |
| `PS1_HEAPLOG=1` | The heap's largest free block before the game module loads. |
| `PS1_FRAMES=1` | Each frame's field and pending ticks. |
| `PS1_SPINSTAT=1` | Per frame, the longest stretch of code without a host call. |
| `PS1_IDLELOG=1`, `PS1_IDLE_DUMP=FILE` | Where the game idles; RAM after many idles. |
| `PS1_STALL_DUMP=FILE` | RAM when no frame has finished for 30 fields. |
| `PS1_SLOWLOG=1` | Host calls slower than 100 ms. |
| `PS1_INTERPLOG=1` | Code the recompiler did not cover (interpreted). |
| `PS1_SAMPLE=1` | Where the CPU is, sampled every 32M branches. |
| `PS1_SNDLOG=1`, `PS1_VOICELOG=1`, `PS1_SPURAM=FILE`, `PS1_DMALOG=1` | SPU key-ons and interrupts, voices, its RAM, DMA. |
| `NFSHS_PCM=FILE` | Also write the mixed audio there (raw 44.1 kHz stereo s16le). |
