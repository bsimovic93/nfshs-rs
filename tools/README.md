# tools

Python helpers for reverse engineering `NFS4.EXE` and checking the rebuild against the
original. Run them from this folder (they import each other: `mips`, `ds`, `recomp`).

## The recompiler

| Script | What it does |
|---|---|
| `recomp.py` | Static recompiler: `NFS4.EXE`'s MIPS functions to Rust (`crates/nfs4rc`). `python recomp.py ../crates/nfs4rc/src --all` |
| `hle_funcs.txt` | Functions replaced on the host side (`src/ps1/host/`): calls to them reach the host. |
| `hook_funcs.txt` | Recompiled functions always called through `Cpu::call`, so the host can observe them. |
| `decomp_funcs.txt` | Functions decompiled to Rust (`crates/nfs4`): their calls ask `Cpu::decomp` first. See [`docs/DECOMP.md`](../docs/DECOMP.md). |
| `gentypes.py` | Typed views of the game's structures for the decompiled code (`crates/nfs4/src/types.rs`), from `re/nfs4_types.h`. |

## Reading the original

| Script | What it does |
|---|---|
| `mips.py` | MIPS-I disassembler and xref helper for the PS-X EXE (used by the others). |
| `fdis.py ADDR [--calls]` | Disassembles a whole retail function. |
| `ramdis.py RAM.bin ADDR COUNT` | Disassembles from a 2 MB RAM dump (overlays such as the front end). |
| `calltree.py` | Call tree of a function. |
| `carprims.py RAM.bin [car]` | A car's GPU packets in its ordering table, from a RAM dump. |
| `mipsemu.py` | Tiny MIPS interpreter running self-contained routines (the codecs' ground truth, `re/dec`). |
| `psxexe2elf.py` | Wraps the PS-X EXE in an ELF so disassemblers map it at its load address. |
| `snsym.py`, `symmatch.py` | Read the 1999 development build's symbols and carry the names over to the retail executable (`re/*.tsv`). |
| `globmatch.py` | The same for the global variables (`re/retail_globals.tsv`), lining up the data each matched function references. |

## The original, running (DuckStation)

| Script | What it does |
|---|---|
| `ds.py` | DuckStation automation: GDB remote protocol and keyboard input to its window. |
| `dsref.py OUT --state NAME --keys ...` | Load a save state from `re/states`, drive it, capture screenshots and RAM (for `nfshs ref-compare`). |
| `dsshot.py` | Captures the DuckStation window to PNG. |
| `appdrive.py EXE OUT KEYS` | Drives the rebuild's window with keys and captures it. |

## archive/

Kept as history; nothing uses them now, and they would need `..` on `PYTHONPATH` to import
`ds` or `mips`.

- **The snapshot emulator** (before the recompiler): `snap.py`, `snapwait.py`, `initcap.py` and
  `stagediff.py` froze DuckStation at a function and saved RAM. Then `snapemu.py` (with `hle.py`,
  `explore.py`, `deps.py`, `export_snap.py`) ran that one function in Python and compared the
  results.
- **Watching DuckStation**: `watch.py` (write watchpoints), `sample.py` (RAM sampling over time),
  `ramdump.py` (RAM over GDB; `dsref.py`'s `ram:` step does this now), `nav.py` (key sequences
  and a screenshot; `dsref.py` does this now), `hudprims.py` (the HUD's primitive buffer from
  RAM).
- **Early format work**: `grpblk.py`, `grptree.py`, `grpwalk.py` (track GRP chunk dumpers) and
  `viv.py` (VIV archives). `src/formats` replaced them.
