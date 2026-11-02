"""Typed views of the game's structures in PS1 RAM, for the decompiled code (crates/nfs4).

Reads the 1999 development build's layouts (re/nfs4_types.h, from NFS4.SYM) and writes
crates/nfs4/src/types.rs: per structure a view type holding its address, with a getter and a
setter per field, sub-views for nested structures and arrays, and views for pointers to the
structures generated. Also a field table, so the shadow checks can name a RAM address.

The retail layouts are the development build's for everything ported so far; a field that
differs goes in OVERRIDES.

usage: gentypes.py [OUT.rs]
"""
import re
import sys

HEADER = '../re/nfs4_types.h'
OUT = sys.argv[1] if len(sys.argv) > 1 else '../crates/nfs4/src/types.rs'

# The structures the ported code uses; whatever they hold by value comes with them.
ROOTS = ['Car_tObj', 'Physics_tWheelAccStruct', 'Car_tSpecs', 'Udff_tInfo', 'BWorldSm_Pos', 'Trk_NewSlice', 'GameSetup_tData', 'Chunk', 'Trk_NewStrip', 'Trk_NewSimSlice', 'Trk_NewSimQuad', 'tNormalCacheEntry', 'AICop_spikeBelt_t',
         'GameSetup_tCarData', 'AIPerson_t', 'AIDataRecord_t', 'AISpeeds_tSlotInfo', 'AISpeeds_tLeaderBoard', 'AIPhysic_BrakeInfo', 'AIPhysic_Config_t', 'AIDelayCar', 'AITrigger_TriggerManager', 'trigger_t', 'AIState_Chase', 'AIState_Idle', 'AIState_Offroad', 'AIState_Purgatory', 'AIState_RovingTraffic', 'AIState_Donuts', 'AIState_GotoSlice', 'AIState_Cruise', 'AIState_Normal', 'AIState_NonActive', 'AIState_None', 'AIHigh_Base', 'AIHigh_None', 'AIHigh_BasicPerp', 'AIHigh_BasicCop', 'AIHigh_BTC_Cop', 'AIHigh_BTC_HumanCop', 'AIHigh_BTC_Wingman', 'AIHigh_BTC_Perp', 'AIHigh_BTC_HumanPerp', 'AIHigh_BTC_AIPerp', 'AIHigh_Player', 'AIHigh_Human', 'AIHigh_Opponent', 'AIHigh_Cop', 'AIHigh_Traffic', 'Input_tResults', 'Sched_tSchedule', 'sim_queue', 'Sim_tSimGlobalVar', 'Sim_tSimSystemVar', 'tReplayInterface', 'tCarManager', 'tCarInfo', 'tSaveCarInfo', 'tListIteratorCar', 'tListIteratorCarColor', 'tfrontEnd', 'DRAWENV']
# The front end's classes (tMenu, tScreenCarSelect...): all of them.
FRONT_END_CLASSES = r't[A-Z]'

# Rust names for some structures (others: underscores and the `_t` type prefix dropped).
NAMES = {'coorddef': 'Coord', 'matrixtdef': 'Matrix', 'BO_tNewtonObj': 'NewtonObj',
         'BO_tNewtonCollisionInfo': 'NewtonCollisionInfo', 'Physics_tWheelAccStruct': 'WheelAcc',
         'Car_tObj': 'Car', 'CCOORD16': 'Coord16', 'Sim_tSimGlobalVar': 'SimGlobal', 'tReplayInterface': 'ReplayInterface',
         'Sim_tSimSystemVar': 'SimVar', '._96': 'CopPosition', 'tfrontEnd': 'FrontEnd', 'tNormalCacheEntry': 'TNormalCacheEntry'}

# (struct, field) -> (offset, type) where retail differs from the development build, or the
# SYM's type reads wrong (the trigger manager's `trigger_t *triggers_[100]`, 100 addresses).
# The list iterators' bounds are `char` in the SYM, but the original loads and compares them
# unsigned (`lbu`, `sltu`).
OVERRIDES = {('AITrigger_TriggerManager', 'triggers_'): (0x8, 'ulong[100]'),
             ('tListIterator', 'fMinValue'): (0x8, 'uchar'), ('tListIterator', 'fMaxValue'): (0x9, 'uchar'),
             # the help's seven lines' strings (`char *text[7]`)
             ('tDialogHelp', 'text'): (0x94, 'ulong[7]')}

SCALARS = {'char': ('i8', 1), 'uchar': ('u8', 1), 'short': ('i16', 2), 'ushort': ('u16', 2),
           'int': ('i32', 4), 'uint': ('u32', 4), 'long': ('i32', 4), 'ulong': ('u32', 4),
           'float': ('u32', 4)}
KEYWORDS = {'type', 'loop', 'move', 'ref', 'match', 'mod', 'use', 'impl', 'fn', 'struct', 'enum',
            'self', 'super', 'crate', 'where', 'box', 'in', 'as', 'const', 'static', 'dyn',
            'final', 'override', 'abstract', 'macro', 'yield', 'gen', 'try', 'async', 'await'}


def parse(path):
    structs = {}
    cur = None
    for line in open(path, encoding='latin1'):
        m = re.match(r'(struct|union) (\S+) /\* size 0x([0-9a-f]+) \*/ \{', line)
        if m:
            name = m[2]
            cur = None if name in structs else name
            if cur:
                structs[cur] = {'size': int(m[3], 16), 'union': m[1] == 'union', 'fields': []}
            continue
        if line.startswith('};'):
            cur = None
            continue
        m = re.match(r'\s+/\* \+0x([0-9a-f]+) \*/ (.+) (\w+|\.vf);', line)
        if m and cur:
            # a C++ class's vtable pointer (`.vf`): its address
            name, t = ('vtable', 'ulong') if m[3] == '.vf' else (m[3], m[2].strip())
            structs[cur]['fields'].append((int(m[1], 16), t, name))
    return structs


def rust_name(s):
    if s in NAMES:
        return NAMES[s]
    # the front end's classes: tCarManager is CarManager
    s = re.sub(r'^t(?=[A-Z])', '', s)
    s = re.sub(r'_t(?=[A-Z]|$)', '_', s)
    return ''.join(p[:1].upper() + p[1:] for p in s.split('_') if p)


def snake(n):
    s = re.sub(r'([a-z0-9])([A-Z])', r'\1_\2', n)
    s = re.sub(r'([A-Z]+)([A-Z][a-z])', r'\1_\2', s).lower()
    return s + '_' if s in KEYWORDS else s


def classify(t, structs):
    """(kind, base, dims): kind is 'scalar', 'struct', 'ptr' (to a struct), 'raw' (an address)
    or 'enum'; dims the array dimensions."""
    dims = [int(d) for d in re.findall(r'\[(\d+)\]', t)]
    ptr = t.endswith('*') or t.endswith('()')
    base = re.sub(r'\[\d+\]', '', t).rstrip('*()').strip()
    if '*' in base or '(' in base:
        return ('raw', None, [])
    if ptr:
        # a pointer to an array of structures is just an address
        if base.startswith('struct ') and not re.search(r'\[\d+\]\*$', t):
            return ('ptr', base[7:], [])
        return ('raw', None, [])
    if base.startswith('struct ') or base.startswith('union '):
        return ('struct', base.split(' ', 1)[1], dims)
    if base.startswith('enum '):
        return ('enum', None, dims)
    if base in SCALARS:
        return ('scalar', base, dims)
    return ('raw', None, dims)


def closure(structs):
    want = []
    stack = list(ROOTS) + sorted(s for s in structs if re.match(FRONT_END_CLASSES, s))
    while stack:
        s = stack.pop()
        if s in want or s not in structs:
            continue
        want.append(s)
        for _, t, _ in structs[s]['fields']:
            kind, base, _ = classify(t, structs)
            if kind == 'struct':
                stack.append(base)
    return sorted(want, key=rust_name)


def base_class(s, info, have):
    """The C++ base class of `s` (its first member, at +0, named as its type), if any."""
    if not info['fields']:
        return None
    off, t, name = info['fields'][0]
    if off == 0 and t == f'struct {name}' and name in have and re.match(FRONT_END_CLASSES, s):
        return name
    return None


def main():
    structs = parse(HEADER)
    want = closure(structs)
    have = set(want)
    out = ['//! Typed views of the game\'s structures in PS1 RAM (generated by tools/gentypes.py from',
           '//! the development build\'s layouts). Do not edit by hand.',
           '//!',
           '//! A view is the address of one structure; its accessors read and write PS1 RAM, so the',
           "//! recompiled code still running sees every change. A C++ class's view dereferences to",
           "//! its base class's (`Deref`), so the members it inherits are reachable on it.",
           '#![allow(dead_code, clippy::all)]',
           'use ps1rt::Mem;',
           '']
    table = []
    for s in want:
        info = structs[s]
        rn = rust_name(s)
        out.append(f'/// `{s}` ({info["size"]:#x} bytes).')
        out.append('#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]')
        out.append('#[repr(transparent)]')
        out.append(f'pub struct {rn}(pub u32);')
        base = base_class(s, info, have)
        if base:
            # a C++ class's base class is its first member, named as its type: the derived
            # view dereferences to the base's (same address), so its members are reachable
            br = rust_name(base)
            out.append(f'/// `{s}` derives from `{base}`: its members are reachable through it.')
            out.append(f'impl core::ops::Deref for {rn} {{')
            out.append(f'    type Target = {br};')
            out.append(f'    fn deref(&self) -> &{br} {{ unsafe {{ &*(self as *const Self as *const {br}) }} }}')
            out.append('}')
            out.append(f'impl {rn} {{')
            out.append(f'    /// The `{base}` it derives from (the same address).')
            out.append(f'    pub fn base(self) -> {br} {{ {br}(self.0) }}')
            out.append('}')
        out.append(f'impl {rn} {{')
        out.append(f'    pub const SIZE: u32 = {info["size"]:#x};')
        out.append(f'    /// Element `i` of an array of `{s}` starting here.')
        out.append(f'    pub fn at(self, i: u32) -> Self {{ Self(self.0.wrapping_add(i.wrapping_mul(Self::SIZE))) }}')
        fields = []
        seen_names = set()
        for off, t, name in info['fields']:
            off, t = OVERRIDES.get((s, name), (off, t))
            kind, base, dims = classify(t, structs)
            # the SYM dump writes an array of pointers `X *f[n]` as `X[n]* f` (tMenu's 16 items,
            # tFEApplication's menus per player): the front end's classes get them as arrays
            pa = re.fullmatch(r'struct (\w+)((?:\[\d+\])+)\*', t)
            if pa and re.match(FRONT_END_CLASSES, s):
                kind, base, dims = 'ptr', pa[1], [int(d) for d in re.findall(r'\d+', pa[2])]
            # (the front end's classes name their members fName)
            fn = snake(name[1:] if re.match(r't[A-Z]', s) and re.match(r'f[A-Z]', name) else name)
            while fn in seen_names:
                fn += '_'
            seen_names.add(fn)
            doc = f'    /// `+{off:#05x} {t} {name}`'
            count = 1
            for d in dims:
                count *= d
            if kind == 'struct' and base in have:
                sub = rust_name(base)
                esize = structs[base]['size']
                out.append(doc)
                if dims:
                    out.append(f'    pub fn {fn}(self, i: u32) -> {sub} {{ debug_assert!(i < {count}); {sub}(self.0.wrapping_add({off:#x} + i * {esize:#x})) }}')
                else:
                    out.append(f'    pub fn {fn}(self) -> {sub} {{ {sub}(self.0.wrapping_add({off:#x})) }}')
                fields.append((off, esize * count, name, esize if dims else 0, f'Some({want.index(base)})'))
                continue
            if kind == 'scalar':
                rt, size = SCALARS[base]
            elif kind == 'enum':
                rt, size = 'i32', 4
            elif kind == 'ptr' and base in have:
                rt, size = rust_name(base), 4
            else:
                rt, size = 'u32', 4
                if kind == 'raw' and dims:
                    size = 4
            rd = {1: 'r8', 2: 'r16', 4: 'r32'}[size]
            wr = {1: 'w8', 2: 'w16', 4: 'w32'}[size]
            ut = {1: 'u8', 2: 'u16', 4: 'u32'}[size]
            if rt in ('i8', 'i16', 'i32'):
                get, put = f'm.{rd}(a) as {rt}', f'm.{wr}(a, v as {ut})'
            elif rt in ('u8', 'u16', 'u32'):
                get, put = f'm.{rd}(a)', f'm.{wr}(a, v)'
            else:
                get, put = f'{rt}(m.r32(a))', 'm.w32(a, v.0)'
            if dims:
                addr = f'let a = self.0.wrapping_add({off:#x} + i * {size}); debug_assert!(i < {count});'
                args = ', i: u32'
            else:
                addr = f'let a = self.0.wrapping_add({off:#x});'
                args = ''
            out.append(doc)
            out.append(f'    pub fn {fn}(self, m: &mut Mem{args}) -> {rt} {{ {addr} {get} }}')
            out.append(f'    pub fn set_{fn}(self, m: &mut Mem{args}, v: {rt}) {{ {addr} {put} }}')
            if rt in ('i8', 'i16', 'i32', 'u8', 'u16', 'u32'):
                iarg = ', i' if dims else ''
                out.append(f'    pub fn update_{fn}(self, m: &mut Mem{args}, f: impl FnOnce({rt}) -> {rt}) {{ let v = f(self.{fn}(m{iarg})); self.set_{fn}(m{iarg}, v) }}')
            fields.append((off, size * count, name, size if dims else 0, 'None'))
        out.append('}')
        out.append('')
        table.append((s, info['size'], fields))
    out.append('/// A field of a structure: offset, size, name, element size (arrays), structure (an index')
    out.append('/// into [`STRUCTS`]) when it is one.')
    out.append('pub struct Field { pub off: u32, pub size: u32, pub name: &\'static str, pub elem: u32, pub sub: Option<usize> }')
    out.append('/// The structures above: name, size and fields.')
    out.append('pub static STRUCTS: &[(&str, u32, &[Field])] = &[')
    for s, size, fields in table:
        fs = ', '.join(f'Field {{ off: {o:#x}, size: {sz:#x}, name: "{n}", elem: {e:#x}, sub: {sub} }}' for o, sz, n, e, sub in fields)
        out.append(f'    ("{s}", {size:#x}, &[{fs}]),')
    out.append('];')
    out.append('')
    out.append('/// The index of structure `name` in [`STRUCTS`].')
    out.append('pub fn struct_index(name: &str) -> Option<usize> { STRUCTS.iter().position(|s| s.0 == name) }')
    out.append('')
    open(OUT, 'w', encoding='utf-8', newline='\n').write('\n'.join(out))
    print(f'{len(want)} structures -> {OUT}')


if __name__ == '__main__':
    main()
