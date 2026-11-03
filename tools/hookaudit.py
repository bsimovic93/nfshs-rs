"""Find decompiled code (crates/nfs4) calling a host-hooked function without going through
Cpu::call, so the host never sees the call.

The host intercepts some of the game's functions to note what it does (the showroom's car,
captured primitives, the autosave between front-end frames...). Recompiled code reaches them
through Cpu::call; a port must call them by address too (Saved::call_ptr / call_ptr_leaving:
Cpu::call runs the host, then routes to the port if there is one). Calling the port directly
(port(), Saved::enter, an `*_entry` fn) or the recompiled original (nfs4rc::f_*) skips the
host, and `nfshs decomp-check` can't tell: it compares RAM, which the hooks don't change.

Hooked addresses come from tools/hook_funcs.txt, tools/hle_funcs.txt, the host's match arms
and `addr ==` checks (by hex or by a `const NAME: u32` in src/ps1) and CAPTURED_FE_DRAWING.

It also finds ports calling the recompiled original (nfs4rc::f_*) of a function that is
decompiled too (in nfs4::FUNCS): routing happens at call sites, so from there the original runs
and the port is never used (nor checked). Call it by address (call_addr, Saved::call_ptr) or,
from readable code, as Rust.

    python tools/hookaudit.py [ROOT]   (exit status 1 when it finds any)
"""
import glob
import os
import re
import sys

ROOT = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ADDR = r'800[0-9A-Fa-f_]{5,7}'


def norm(a):
    return int(a.replace('_', ''), 16)


def read(p):
    return open(p, encoding='utf8').read()


def code_lines(p):
    """The file's lines, numbered from 1, with comments and its test modules blanked (a file can
    go on after one: screencarselect.rs has two)."""
    lines = [re.sub(r'//.*', '', l) for l in read(p).split('\n')]
    i = 0
    while i < len(lines):
        if lines[i].strip() == '#[cfg(test)]' and i + 1 < len(lines) and re.match(r'\s*(pub(\(crate\))? )?mod \w+ \{', lines[i + 1]):
            depth, j = 0, i + 1
            while j < len(lines):
                depth += lines[j].count('{') - lines[j].count('}')
                lines[j] = ''
                j += 1
                if depth <= 0:
                    break
            i = j
        else:
            i += 1
    return list(enumerate(lines, 1))


def hooked_functions():
    hooked = {}
    for f in ('tools/hook_funcs.txt', 'tools/hle_funcs.txt'):
        for l in open(os.path.join(ROOT, f), encoding='utf8'):
            if l.strip() and not l.startswith('#'):
                a, rest = l.split(None, 1)
                hooked[int(a, 16)] = f'{os.path.basename(f)}: {rest.strip()}'
    consts = {}
    for p in glob.glob(os.path.join(ROOT, 'src/ps1/**/*.rs'), recursive=True):
        for m in re.finditer(r'const (\w+): u32 = 0x(' + ADDR + r');', read(p)):
            consts[m.group(1)] = norm(m.group(2))
    for p in glob.glob(os.path.join(ROOT, 'src/ps1/host/*.rs')):
        where = os.path.basename(p)
        for i, l in enumerate(read(p).split('\n'), 1):
            for m in re.finditer(r'\b0x(' + ADDR + r')\s*(?:=>|\|)', l):
                hooked.setdefault(norm(m.group(1)), f'{where}:{i}')
            for m in re.finditer(r'addr == 0x(' + ADDR + r')', l):
                hooked.setdefault(norm(m.group(1)), f'{where}:{i}')
            for m in re.finditer(r'^\s*(?:\w+::)?([A-Z][A-Z0-9_]+)\s*(?:if .*?)?(?:=>|\|)', l):
                if m.group(1) in consts:
                    hooked.setdefault(consts[m.group(1)], f'{where}:{i} {m.group(1)}')
            for m in re.finditer(r'addr == (?:\w+::)?([A-Z][A-Z0-9_]+)', l):
                if m.group(1) in consts:
                    hooked.setdefault(consts[m.group(1)], f'{where}:{i} {m.group(1)}')
    cap = re.search(r'CAPTURED_FE_DRAWING[^=]*=\s*\[(.*?)\];', read(os.path.join(ROOT, 'src/ps1/host/mod.rs')), re.S)
    if cap:
        for m in re.finditer(r'0x([0-9A-Fa-f_]+)', cap.group(1)):
            hooked.setdefault(norm(m.group(1)), 'CAPTURED_FE_DRAWING')
    return hooked


def main():
    hooked = hooked_functions()
    files = sorted(glob.glob(os.path.join(ROOT, 'crates/nfs4/src/*.rs')))
    # `fn x_entry(c) { abi::leave_prologue(c, ADDR..` helpers enter the port at ADDR
    entries = {}
    for p in files:
        for m in re.finditer(r'fn (\w+_entry)\(c: &mut Cpu\)\s*\{\s*(?:abi::)?leave_prologue\(c, 0x([0-9A-Fa-f_]+)', read(p)):
            entries[m.group(1)] = norm(m.group(2))
    # the ports of hooked functions, by their `abi_*` names
    ported = {}
    for m in re.finditer(r'func(?:_ret)?\(0x([0-9A-Fa-f_]+), "[^"]+", (\w+)::(\w+)\)', read(os.path.join(ROOT, 'crates/nfs4/src/lib.rs'))):
        if norm(m.group(1)) in hooked:
            ported[(m.group(2), m.group(3))] = norm(m.group(1))

    routed = {norm(m.group(1)) for m in re.finditer(r'func(?:_ret)?\(0x([0-9A-Fa-f_]+), "', read(os.path.join(ROOT, 'crates/nfs4/src/lib.rs')))}
    found = unrouted = 0
    for p in files:
        here = os.path.splitext(os.path.basename(p))[0]
        if here == 'lib':
            continue
        for i, l in code_lines(p):
            hits = [(int(m.group(1), 16), 'the recompiled original') for m in re.finditer(r'nfs4rc::f_([0-9a-f]{8})\b', l)]
            # port helpers and Saved::enter*: (c, ADDR, ..) or (c, &r, ADDR, ..)
            hits += [(norm(m.group(1)), 'its port') for m in re.finditer(r'\b(?:port\w*|enter\w*)\(\s*c,\s*(?:&?(?!0x)\w+,\s*)?0x([0-9A-Fa-f_]{8,10})\b', l)]
            hits += [(entries[m.group(1)], m.group(1)) for m in re.finditer(r'\b(\w+_entry)\b', l)
                     if m.group(1) in entries and not re.search(r'fn ' + m.group(1) + r'\b', l)]
            # a port's `abi_*` fn: unqualified in its own module, `module::abi_*` elsewhere
            for m in re.finditer(r'\b(?:(\w+)::)?(abi_\w+)\b', l):
                key = (m.group(1) or here, m.group(2))
                if key in ported and not re.search(r'fn ' + m.group(2) + r'\b', l):
                    hits.append((ported[key], f'{key[0]}::{key[1]}'))
            for a, how in hits:
                if how == 'the recompiled original' and a in routed and a not in hooked:
                    unrouted += 1
                    print(f'{os.path.relpath(p, ROOT)}:{i}: {a:#x} is decompiled, but called as the recompiled original (the port never runs from here)')
                    print(f'    {l.strip()[:150]}')
                if a in hooked:
                    found += 1
                    print(f'{os.path.relpath(p, ROOT)}:{i}: {a:#x} called through {how}, not by address [{hooked[a]}]')
                    print(f'    {l.strip()[:150]}')
    print(f'{len(hooked)} hooked functions; {found} direct call{"" if found == 1 else "s"} found')
    print(f'{len(routed)} decompiled functions; {unrouted} call{"" if unrouted == 1 else "s"} of their recompiled originals found')
    return 1 if found or unrouted else 0


if __name__ == '__main__':
    sys.exit(main())
