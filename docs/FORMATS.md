# Need for Speed: High Stakes (PS1, NTSC-U SLUS-00826) — file formats

Reverse-engineered from the disc image and `NFS4.EXE` (PS-X EXE, MIPS R3000, loaded at
`0x80010000`, entry `0x800E4C38`). All multi-byte values are little-endian unless noted.
Status: **confirmed** = verified against game code or round-trip/visual checks;
**inferred** = consistent with data but not traced in code; **unknown** = not decoded.

## Disc

Single MODE2/2352 data track, ISO9660. `SYSTEM.CNF` boots `SLUS_008.26` (a 30 KB loader),
which runs `NFS4.EXE` (main game) and the `FRONT.BIN` overlay. File names follow
`%sTr%02d.grp`, `%szzz%s.viv`, etc. (format strings at `0x800559D0..0x80057268`).

| Pattern | Contents |
|---|---|
| `ZTRnn.GRP`, `ZTRnn{N,S,W}.GRP` | track geometry (day / night / snow / wet) |
| `ZTRnn0.PSH`, `ZTRnn{N,S,W}0.PSH` | track textures |
| `ZTRnn.BIN` | lighting presets (sky colours, fog, ambient) — partly decoded |
| `ZTRnn.QBE/.QCR` | compressed AI/route data (decompresses; contents unknown) |
| `ZTRnn.COP`, `.ENV`, `.FOG`, `.KIL` | police routes, env-map slices (text), fog, kill zones — unknown/text |
| `ZZZcode.VIV` | car model + textures (`H` = hi-detail, `S` = ?) |
| `Zcode.QDA`, `Zcode.QCS` | car engine data / speed tables (compressed) |
| `ZcodeENG.VIV` | engine sound bank (`.bnk`) + `CRDl` curve tables (`.ctb/.ltb`) |
| `ZZZZAMUS.VIV`, `ZZZZYMUS.VIV` | front-end / in-race music (`SCHl` streams) |
| `ZZZFE.VIV` | front-end art (title, track and car banners, ...) |
| `ZTEXT.ENG` etc. | localised strings (track and car names) |
| `MOVIES/*.XA` | FMV — not decoded |

## Containers

**VIV / `BIGF`** (confirmed) — big-endian: `"BIGF" u32 size u32 count u32 header_size`,
then `count × {u32 offset, u32 size, char name[] NUL}`.

**`C0FB`** small archives (camera `.can` files in `ZTRnnA.VIV`) — not decoded.

## Compression (confirmed, ported from the game's own routines)

Dispatcher `0x800E4A74(src, dst) -> size`; header `xx FB`, `xx & 0xFE` selects the codec.
If `xx & 1`, a 24-bit compressed size follows the 2-byte id. Then a 24-bit BE unpacked size.

* **0x10 RefPack** (`0x800F5C14`): standard EA LZ (2/3/4-byte copy commands, literal runs, `FC..FF` terminator).
* **0x30/0x32/0x34 Huffman** (`0x800F4EF8`): MSB-first bit stream. `clue` escape byte (8 bits);
  per code length *n* = 1.. a count via `getnum` (`z` zero bits, a one bit, then `z+2` bits *v*
  → `v + 2^(z+2) − 4`) until the canonical code space is full; symbols as skip-counts over
  unused byte values; 8-bit fast lookup + limit table for longer codes. After the clue symbol:
  `getnum` *n* > 0 repeats the previous byte *n* times, *n* = 0 then one bit: 0 = 8-bit literal,
  1 = end. `0x32` applies a running-sum delta, `0x34` a double delta.
* **0x46/0x47 BTree** (`0x800F5F10`): clue byte, node count, `{node, left, right}` triples;
  node bytes expand recursively, clue + 0 ends, clue + *b* emits literal *b*.

All codecs reproduce the emulated originals byte-for-byte (`nfshs verify-decompress`).

## Textures — `SHPP` / `GIMX` (`.PSH`) (confirmed)

`"SHPP" u32 size u32 count "GIMX"`, then `count × {char name[4], u32 offset}`. Each entry is
a chain of 16-byte block headers: `u8 type, u24 next (relative, 0 = last), u16 w, u16 h,
i16 center_x, i16 center_y, u16 vram_x, u16 vram_y`.

* `0x40` 4bpp, `0x41` 8bpp, `0x42` 16bpp (PS1 BGR555, `0x0000` = transparent), `0xC1` 8bpp variant.
* Rows are padded to whole 16-bit VRAM words (4 px at 4bpp, 2 px at 8bpp).
* `0x23` block = 16-bit CLUT for the preceding image.
* `vram_x` is in 16-bit units (×4 px at 4bpp). Car sheets tile one 256×256 texture page,
  which car UVs address directly. Front-end images are split into tiles positioned by
  `(-center_x, -center_y)`.

## Car models — `.GEO` (confirmed)

A memory image with absolute pointers: `u32 part_ptr[57]`. The load base is recovered from
the first part, whose vertex pointer points just past its 0x1C-byte header.
Part: `u16 nverts, u16 npolys, i32 pos[3] (16.16 m), u32 verts, u32 normals, u32 polys`.
Vertices/normals: `i16 xyz` (vertices 1/256 m, normals 4.12). Polygon (12 bytes):
`u8 flags, u8 flags2 (0x80 = mirrored half), u8 texture, u8 v[3], u8 uv[3][2]`.
Y is up, +Z forward, origin at ground level. Parts: 0 = hi body, 1 = low body,
41/42 = body halves, 43/44 = mirrors, 47/48 + 51/52 = front wheels (hub + tyre),
55/56 = rear wheels, others = lights, decals, light bars (inferred).

## Tracks — `.GRP` (confirmed unless noted)

Chunk tree, each chunk `u32 type, u32 size (incl. 16-byte header), u32 0xCDCDCDCD, u32 count`.

* `0x1E` root → `0x1F` info, `0x20` nodes (`i32 xyz` 16.16 m, one per segment block),
  `0x1D` segment blocks, `0x21` globals, `0x23` colour table.
* Segment `0x1D`: `0x1C` header (bounding radius + outline), `0x17` geometry, `0x03` object
  placements, `0x0B` emitter, `0x04/05/06/09/0A` visibility/AI (unknown).
* Geometry `0x17`: `0x1B` counts (`[6]` quads in 0x19, `[8]` 0x28, `[9]` 0x29); `0x18` and
  `0x27` vertex pools `{i16 x,y,z, u16 shade}`, world = node + v/64; `0x19/0x28/0x29` quads
  `{u16 tex, u8 v[4]}`; `0x1A/0x25` strip rows `{u8 next_row, u8 row, u8 n, u8 2(n+2), u16 tex[n]}`.
* Texture table `0x21/0x02`, 10-byte entries: `u16 shape, u8 mode, u8 flags, u8 rgb[3], u8, u16`.
  Flags: `0x08` flip U, `0x04` flip V, `0x02` rotate 90° (confirmed visually);
  `0x01/0x10/0x40` base modes (unknown, probably transparency/lighting).
  Each quad maps the whole shape.
* Prop models `0x21/0x08`: `{u16 flags, u8 nv, u8 nq}`, then vertices and quads as above, padded to 4.
* Placements (`0x03`, length-prefixed records): `u8 len, u8 0, u8 type, u8 sub, u16 ?, u16 model,
  i32 pos[3]`. Type 5 (36 bytes) adds `i32 cos, i32 sin` (2.30) and `u16 scale[3]` (0x100 = 1.0);
  type 9 (28 bytes) adds `i16 ?, u16 sx, u16 sy, u16 flags`. Flat single-quad models are
  camera-facing sprites (trees).
* Virtual road `0x21/0x0F` (32-byte samples, about 6 m apart): `i32 pos[3]`, `i8 normal[3]`,
  `i8 forward[3]`, `i8 right[3]`, `u8`, `u16 flags`, `u16 left_wall`, `u16 right_wall`
  (1/256 m), `u8 block, u8 ?, u8 ?, u8 ?`. These walls are the drivable boundary.
* The PS1 world is left-handed; the engine mirrors X.

## Car engine data — `.QDA` (decompressed, 276 bytes) (inferred, consistent across cars)

`+0x00` redline (16.16 rpm); `+0x04..0x1C` road speed at redline per gear (16.16 m/s,
0 = unused gear); `+0x20` `i16[112]` net full-throttle acceleration in 1/256 m/s² per 1 m/s of
road speed (its zero crossing is the top speed); `+0x100` constants. `.QCS` = 256-byte
decreasing table (probably speed-dependent steering or AI limits).

## Audio (confirmed)

EA "PT" patch headers: `"PT" u8 platform(1 = PS1) u8 0`, then tag/len/BE-value records
(`0xFC..0xFE` have no length, `0xFF` ends). Tags: `0x82` channels, `0x84` rate (default 22050),
`0x85` samples, `0x86/0x87` loop, `0x88` data offset.

* `BNKl` banks: `"BNKl" u16 ver(4) u16 count`, PT header pointers at `0x14 + 4i` (relative to
  the pointer). Data is PS1 ADPCM; stereo entries interleave 16-byte frames.
* `SCHl` streams: `SCHl`(+u32, PT) `SCCl` then `SCDl` blocks `{u32 ?, u32 samples,
  ch0 frames, ch1 frames}` and `SCEl`.
* `CRDl` (`.ctb/.ltb`): rpm→volume/pitch curves for engine layers (u8 curves, 100 steps) — not used yet.

## Text configs

`ZTRACK.DAT` (shadow/horizon colours per track and condition), `ZCARMAP.DAT` (env-map and
ride height per car, and the game's internal car order), `ZTRnn.ENV` — plain text with C comments.
