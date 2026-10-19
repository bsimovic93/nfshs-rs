"""Static recompiler: MIPS R3000 functions from NFS4.EXE -> Rust source.

Each MIPS function becomes `pub fn f_XXXXXXXX(c: &mut Cpu)`, a state machine over basic blocks
with exact delay-slot semantics. Calls between recompiled functions are direct; others go
through `c.call(addr)` (which must be provided by the host).

usage: recomp.py OUT.rs ROOT_FUNC...
"""
import sys
sys.path.insert(0, __file__.rsplit('\\', 1)[0] if '\\' in __file__ else '.')
import mips

e = mips.Exe('../gamedata/NFS4.EXE')
# The front-end overlay (FRONT.BIN) stays resident at 0x80010000 during races and race code
# calls into it. NFS4.EXE has no code in that range (start-up rdata only), so recompile the
# combined image: executable with the overlay laid over its low region.
_front = open('../gamedata/FRONT.BIN', 'rb').read()
_img = bytearray(e.data)
_img[0:len(_front)] = _front
e.data = bytes(_img)

# all direct call targets in the executable -> function starts
ALL_CALLS = set()
for i in range(len(e.data) // 4):
    pc = e.base + 4 * i
    w = e.word(pc)
    if w >> 26 == 3:
        ALL_CALLS.add((pc & 0xF0000000) | ((w & 0x3FFFFFF) << 2))
SORTED_STARTS = sorted(ALL_CALLS)


def is_prologue(w):
    return (w >> 16) == 0x27BD and (w & 0x8000)


_END_CACHE = {}


def func_end(start):
    """Function extent by control-flow tracing from the entry: follow branches and jumps,
    stop paths at `jr ra`. A `jr reg` (jump table) extends linearly to the next known start."""
    if start in _END_CACHE:
        return _END_CACHE[start]
    import bisect
    k = bisect.bisect_right(SORTED_STARTS, start)
    limit = min(SORTED_STARTS[k] if k < len(SORTED_STARTS) else e.end, e.end, start + 0x8000)
    seen = set()
    work = [start]
    hi = start
    jump_table = False
    while work:
        pc = work.pop()
        while start <= pc < limit and pc not in seen:
            seen.add(pc)
            hi = max(hi, pc)
            w = e.word(pc)
            op = w >> 26
            fn = w & 63
            if op == 0 and fn == 8:  # jr
                seen.add(pc + 4); hi = max(hi, pc + 4)
                if ((w >> 21) & 31) != 31:
                    jump_table = True
                break
            if op == 2:  # j
                t = (pc & 0xF0000000) | ((w & 0x3FFFFFF) << 2)
                seen.add(pc + 4); hi = max(hi, pc + 4)
                if start <= t < limit:
                    work.append(t)
                break
            if op in (4, 5, 6, 7) or op == 1:
                s16 = w & 0xFFFF
                s16 = s16 - 0x10000 if s16 & 0x8000 else s16
                t = (pc + 4 + s16 * 4) & 0xFFFFFFFF
                if start <= t < limit:
                    work.append(t)
                # unconditional "b" (beq zero, zero) ends the path
                if op == 4 and ((w >> 21) & 31) == 0 and ((w >> 16) & 31) == 0:
                    seen.add(pc + 4); hi = max(hi, pc + 4)
                    break
            pc += 4
    end = hi + 4
    if jump_table:
        end = limit
    _END_CACHE[start] = end
    return end


def looks_like_function(a):
    """Address-taken code constant that plausibly starts a function."""
    if not (e.valid(a) and a & 3 == 0):
        return False
    if a in ALL_CALLS:
        return True
    # a stack frame set up within the first instructions (the compiler schedules loads
    # before it), or the previous function's return just before (with alignment padding)
    if any(e.valid(a + 4 * k) and is_prologue(e.word(a + 4 * k)) for k in range(4)):
        return True
    return any(e.valid(a - 4 * k) and e.word(a - 4 * k) == 0x03E00008 for k in (2, 3))


def calls_of(start):
    end = func_end(start)
    out = set()
    hi = {}
    for q in range(start, end, 4):
        w = e.word(q)
        op = w >> 26
        if op == 3:
            out.add((q & 0xF0000000) | ((w & 0x3FFFFFF) << 2))
        rt, rs = (w >> 16) & 31, (w >> 21) & 31
        if op == 15:
            hi[rt] = (w & 0xFFFF) << 16
        elif op in (9, 13) and rs in hi:
            imm = w & 0xFFFF
            v = (hi[rs] + (imm - 0x10000 if (op == 9 and imm & 0x8000) else imm)) & 0xFFFFFFFF
            if 0x80010000 <= v < e.end and looks_like_function(v):
                out.add(v)
    return out


HLE_SET = set()

# Optional names transferred from the 1999 development build (tools/symmatch.py output).
NAMES = {}
SRC = {}
try:
    for line in open('../re/retail_syms.tsv'):
        r, n, d = line.rstrip(chr(10)).split(chr(9))
        NAMES[int(r, 16)] = (n, int(d, 16))
    for line in open('../re/nfs4_funcs.tsv', encoding='latin1'):
        a, loc, n = line.split(chr(9))[:3]
        SRC[int(a, 16)] = loc.split('nfs4' + chr(92))[-1]
except FileNotFoundError:
    pass



def collect(roots):
    seen = set()
    stack = list(roots)
    while stack:
        f = stack.pop()
        if f in seen or f in HLE_SET or not e.valid(f):
            continue
        seen.add(f)
        stack.extend(calls_of(f))
    return sorted(seen)


R = ['zero', 'at', 'v0', 'v1', 'a0', 'a1', 'a2', 'a3', 't0', 't1', 't2', 't3', 't4', 't5', 't6', 't7',
     's0', 's1', 's2', 's3', 's4', 's5', 's6', 's7', 't8', 't9', 'k0', 'k1', 'gp', 'sp', 'fp', 'ra']


def reg(i):
    return f'c.r[{i}]'


def set_reg(i, expr):
    if i == 0:
        return f'let _ = {expr};'
    return f'c.r[{i}] = {expr};'


def sx(imm):
    return imm - 0x10000 if imm & 0x8000 else imm


def gen_simple(w, pc):
    """Rust for a non-control-flow instruction (or None if control flow)."""
    op = w >> 26
    rs, rt, rd = (w >> 21) & 31, (w >> 16) & 31, (w >> 11) & 31
    sh, fn = (w >> 6) & 31, w & 63
    imm = w & 0xFFFF
    s = sx(imm)
    if w == 0:
        return ''
    if op == 0:
        if fn == 0: return set_reg(rd, f'{reg(rt)} << {sh}')
        if fn == 2: return set_reg(rd, f'{reg(rt)} >> {sh}')
        if fn == 3: return set_reg(rd, f'(({reg(rt)} as i32) >> {sh}) as u32')
        if fn == 4: return set_reg(rd, f'{reg(rt)}.wrapping_shl({reg(rs)} & 31)')
        if fn == 6: return set_reg(rd, f'{reg(rt)}.wrapping_shr({reg(rs)} & 31)')
        if fn == 7: return set_reg(rd, f'(({reg(rt)} as i32).wrapping_shr({reg(rs)} & 31)) as u32')
        if fn == 16: return set_reg(rd, 'c.hi')
        if fn == 18: return set_reg(rd, 'c.lo')
        if fn == 17: return f'c.hi = {reg(rs)};'
        if fn == 19: return f'c.lo = {reg(rs)};'
        if fn == 24: return f'c.mult({reg(rs)}, {reg(rt)});'
        if fn == 25: return f'c.multu({reg(rs)}, {reg(rt)});'
        if fn == 26: return f'c.div({reg(rs)}, {reg(rt)});'
        if fn == 27: return f'c.divu({reg(rs)}, {reg(rt)});'
        if fn in (32, 33): return set_reg(rd, f'{reg(rs)}.wrapping_add({reg(rt)})')
        if fn in (34, 35): return set_reg(rd, f'{reg(rs)}.wrapping_sub({reg(rt)})')
        if fn == 36: return set_reg(rd, f'{reg(rs)} & {reg(rt)}')
        if fn == 37: return set_reg(rd, f'{reg(rs)} | {reg(rt)}')
        if fn == 38: return set_reg(rd, f'{reg(rs)} ^ {reg(rt)}')
        if fn == 39: return set_reg(rd, f'!({reg(rs)} | {reg(rt)})')
        if fn == 42: return set_reg(rd, f'(({reg(rs)} as i32) < ({reg(rt)} as i32)) as u32')
        if fn == 43: return set_reg(rd, f'({reg(rs)} < {reg(rt)}) as u32')
        if fn in (8, 9, 12, 13):
            return None
        raise ValueError(f'special {fn} at {pc:#x}')
    if op == 16:
        if rs == 0: return set_reg(rt, f'c.cop0[{rd}]')
        if rs == 4: return f'c.cop0[{rd}] = {reg(rt)};'
        if rs == 16 and fn == 16: return '/* rfe */'
        raise ValueError(f'cop0 {w:#010x} at {pc:#x}')
    if op in (18, 50, 58):
        return f'c.gte({w:#010x});'
    if op in (8, 9): return set_reg(rt, f'{reg(rs)}.wrapping_add({s & 0xFFFFFFFF:#x})')
    if op == 10: return set_reg(rt, f'(({reg(rs)} as i32) < {s}) as u32')
    if op == 11: return set_reg(rt, f'({reg(rs)} < {s & 0xFFFFFFFF:#x}) as u32')
    if op == 12: return set_reg(rt, f'{reg(rs)} & {imm:#x}')
    if op == 13: return set_reg(rt, f'{reg(rs)} | {imm:#x}')
    if op == 14: return set_reg(rt, f'{reg(rs)} ^ {imm:#x}')
    if op == 15: return set_reg(rt, f'{imm << 16:#x}')
    addr = f'{reg(rs)}.wrapping_add({s & 0xFFFFFFFF:#x})'
    if op == 32: return set_reg(rt, f'c.m.r8({addr}) as i8 as i32 as u32')
    if op == 33: return set_reg(rt, f'c.m.r16({addr}) as i16 as i32 as u32')
    if op == 35: return set_reg(rt, f'c.m.r32({addr})')
    if op == 36: return set_reg(rt, f'c.m.r8({addr}) as u32')
    if op == 37: return set_reg(rt, f'c.m.r16({addr}) as u32')
    if op == 34: return set_reg(rt, f'c.lwl({addr}, {reg(rt)})')
    if op == 38: return set_reg(rt, f'c.lwr({addr}, {reg(rt)})')
    if op == 40: return f'c.m.w8({addr}, {reg(rt)} as u8);'
    if op == 41: return f'c.m.w16({addr}, {reg(rt)} as u16);'
    if op == 43: return f'c.m.w32({addr}, {reg(rt)});'
    if op == 42: return f'c.swl({addr}, {reg(rt)});'
    if op == 46: return f'c.swr({addr}, {reg(rt)});'
    if op in (1, 2, 3, 4, 5, 6, 7):
        return None
    raise ValueError(f'op {op} at {pc:#x} ({w:#010x})')


def branch_info(w, pc):
    """(kind, cond_expr, target) for control flow instructions."""
    op = w >> 26
    rs, rt = (w >> 21) & 31, (w >> 16) & 31
    s = sx(w & 0xFFFF)
    tgt = (pc + 4 + s * 4) & 0xFFFFFFFF
    if op == 0 and (w & 63) == 8:
        return ('jr', rs, None)
    if op == 0 and (w & 63) == 9:
        return ('jalr', rs, (w >> 11) & 31)
    if op == 0 and (w & 63) == 12:
        return ('syscall', None, None)
    if op == 2:
        return ('j', None, (pc & 0xF0000000) | ((w & 0x3FFFFFF) << 2))
    if op == 3:
        return ('jal', None, (pc & 0xF0000000) | ((w & 0x3FFFFFF) << 2))
    if op == 4:
        return ('b', f'{reg(rs)} == {reg(rt)}', tgt)
    if op == 5:
        return ('b', f'{reg(rs)} != {reg(rt)}', tgt)
    if op == 6:
        return ('b', f'({reg(rs)} as i32) <= 0', tgt)
    if op == 7:
        return ('b', f'({reg(rs)} as i32) > 0', tgt)
    if op == 1:
        cond = f'({reg(rs)} as i32) >= 0' if rt & 1 else f'({reg(rs)} as i32) < 0'
        if rt & 16:
            return ('bal', cond, tgt)
        return ('b', cond, tgt)
    return None


# Addresses other functions continue at inside a function's range (their branch targets past
# their own end, and the fall-through after a branch in their last slot): each needs its own
# block in the function holding it, or the continuation would find no entry.
ENTRY_POINTS = set()


def collect_entry_points(funcs):
    for g in funcs:
        end = func_end(g)
        for pc in range(g, end, 4):
            bi = branch_info(e.word(pc), pc)
            if not bi:
                continue
            if bi[0] in ('b', 'bal', 'j') and not g <= bi[2] < end:
                ENTRY_POINTS.add(bi[2])
            if bi[0] in ('b', 'bal') and pc + 8 >= end:
                ENTRY_POINTS.add(pc + 8)


def gen_function(f, known):
    end = func_end(f)
    words = {pc: e.word(pc) for pc in range(f, end, 4)}
    has_indirect = any((w >> 26) == 0 and (w & 63) == 8 and ((w >> 21) & 31) != 31 for w in words.values())
    leaders = {f}
    for pc, w in words.items():
        bi = branch_info(w, pc)
        if bi:
            kind = bi[0]
            if kind in ('b', 'bal') and f <= bi[2] < end:
                leaders.add(bi[2])
            if kind == 'j' and f <= bi[2] < end:
                leaders.add(bi[2])
            leaders.add(pc + 8)
    leaders |= {x for x in ENTRY_POINTS if f <= x < end}
    if has_indirect:
        leaders |= set(words)
    leaders = sorted(x for x in leaders if f <= x < end)
    doc = []
    if f in NAMES:
        n, dev = NAMES[f]
        doc.append(f'/// `{n}`' + (f' ({SRC[dev]})' if dev in SRC else ''))
    out = doc + [f'pub fn f_{f:08x}(c: &mut Cpu) {{ f_{f:08x}_at(c, {f:#x}) }}',
                 f'pub fn f_{f:08x}_at(c: &mut Cpu, mut pc: u32) {{', f'    c.stack.push({f:#x});', '    loop {', '        match pc {']
    for li, start in enumerate(leaders):
        stop = leaders[li + 1] if li + 1 < len(leaders) else end
        body = []
        pc = start
        terminated = False
        while pc < stop:
            w = words[pc]
            bi = branch_info(w, pc)
            if bi is None:
                code = gen_simple(w, pc)
                if code:
                    body.append(f'                {code} // {pc:#x}: {mips.dis1(w, pc)}')
                pc += 4
                continue
            kind = bi[0]
            # (a branch at the end of the range still has its delay slot, even where the next
            # function starts: it runs before the jump either way)
            delay_w = words.get(pc + 4, e.word(pc + 4))
            delay = gen_simple(delay_w, pc + 4) or ''
            if delay is None:
                raise ValueError(f'branch in delay slot at {pc:#x}')
            dl = f'                {delay} // delay {pc + 4:#x}' if delay else ''
            comment = f' // {pc:#x}: {mips.dis1(w, pc)}'
            if kind == 'b':
                body.append(f'                let cond = {bi[1]};{comment}')
                if dl: body.append(dl)
                tick = 'c.tick(); ' if bi[2] <= pc else ''
                body.append(f'                if cond {{ {tick}pc = {bi[2]:#x}; continue; }}')
                body.append(f'                pc = {pc + 8:#x}; continue;')
            elif kind == 'bal':
                raise ValueError(f'bltzal/bgezal at {pc:#x}')
            elif kind == 'j':
                if dl: body.append(dl)
                if f <= bi[2] < end:
                    tick = 'c.tick(); ' if bi[2] <= pc else ''
                    body.append(f'                {tick}pc = {bi[2]:#x}; continue;{comment}')
                else:  # tail call
                    body.append(f'                c.stack.pop(); {call_expr(bi[2], known)} return;{comment}')
            elif kind == 'jal':
                if dl: body.append(dl)
                body.append(f'                c.r[31] = {pc + 8:#x};{comment}')
                body.append(f'                {call_expr(bi[2], known)} if c.cont != 0 {{ c.run_cont(); }}')
                body.append(f'                pc = {pc + 8:#x}; continue;')
            elif kind == 'jr':
                rs = bi[1]
                body.append(f'                let target = {reg(rs)};{comment}')
                if dl: body.append(dl)
                if rs == 31:
                    body.append('                let _ = target; c.stack.pop(); return;')
                else:
                    body.append(f'                if ({f:#x}..{end:#x}).contains(&target) {{ pc = target; continue; }}')
                    body.append('                c.stack.pop(); c.call(target); return;')
            elif kind == 'jalr':
                rs, rd = bi[1], bi[2]
                body.append(f'                let target = {reg(rs)};{comment}')
                if dl: body.append(dl)
                if rd: body.append(f'                c.r[{rd}] = {pc + 8:#x};')
                body.append('                c.call(target); if c.cont != 0 { c.run_cont(); }')
                body.append(f'                pc = {pc + 8:#x}; continue;')
            elif kind == 'syscall':
                body.append(f'                c.syscall();{comment}')
                pc += 4
                continue
            terminated = True
            pc += 8
            break
        if not terminated:
            body.append(f'                pc = {stop:#x}; continue;')
        out.append(f'            {start:#x} => {{')
        out.extend(body)
        out.append('            }')
    # control left the traced extent (fall-through into a neighbouring function): tail call
    out.append('            _ => { c.stack.pop(); c.cont = pc; return; }')
    out += ['        }', '    }', '}', '']
    return '\n'.join(out)


HOOK_SET = set()
try:
    for _l in open('hook_funcs.txt'):
        if _l.strip() and not _l.startswith('#'):
            HOOK_SET.add(int(_l.split()[0], 16))
except FileNotFoundError:
    pass


# Functions decompiled to Rust (crates/nfs4): calls to them ask `c.decomp` first and run the
# recompiled original when it declines (it stays generated: the fallback and the reference).
DECOMP_SET = set()
try:
    for _l in open('decomp_funcs.txt'):
        if _l.strip() and not _l.startswith('#'):
            DECOMP_SET.add(int(_l.split()[0], 16))
except FileNotFoundError:
    pass


def call_expr(t, known):
    if t in known and t in DECOMP_SET and t not in HOOK_SET:
        return f'if !(c.decomp)(c, {t:#x}) {{ f_{t:08x}(c); }}'
    return f'f_{t:08x}(c);' if t in known and t not in HOOK_SET else f'c.call({t:#x});'


def all_starts():
    starts = set(ALL_CALLS)
    for a in range(e.base, e.end, 4):
        if is_prologue(e.word(a)):
            starts.add(a)
    for a in range(e.base, e.end, 4):
        v = e.word(a)
        if 0x80010000 <= v < e.end and looks_like_function(v):
            starts.add(v)
    return sorted(x for x in starts if e.valid(x))


def safe_gen(f, known):
    try:
        return gen_function(f, known)
    except ValueError as ex:
        return f'pub fn f_{f:08x}(c: &mut Cpu) {{ let _ = c; panic!("f_{f:08x}: not code"); }}' + chr(10)


if __name__ == '__main__':
    # usage: recomp.py OUT_DIR [--all | ROOTS...] [--hle ADDRS...] [--parts N]
    out_dir = sys.argv[1]
    args = sys.argv[2:]
    if '--hle' in args:
        i = args.index('--hle')
        HLE_SET.update(int(x, 16) for x in args[i + 1:])
        args = args[:i]
    for line in open('hle_funcs.txt'):
        if line.strip() and not line.startswith('#'):
            HLE_SET.add(int(line.split()[0], 16))
    parts_n = 48
    if '--parts' in args:
        i = args.index('--parts'); parts_n = int(args[i + 1]); args = args[:i] + args[i + 2:]
    roots = all_starts() if '--all' in args else [int(x, 16) for x in args]
    funcs = collect(roots)
    known = set(funcs)
    collect_entry_points(funcs)
    import os
    os.makedirs(out_dir, exist_ok=True)
    header = ['//! Generated by tools/recomp.py from NFS4.EXE (NTSC-U SLUS-00826). Do not edit by hand.',
              '#![allow(unused_variables, unused_assignments, unused_mut, clippy::all, unreachable_code, non_snake_case, unused_imports)]',
              'use ps1rt::Cpu;', 'use crate::*;', '']
    chunk = (len(funcs) + parts_n - 1) // parts_n
    for k in range(parts_n):
        sub = funcs[k * chunk:(k + 1) * chunk]
        body = header + [safe_gen(f, known) for f in sub]
        open(os.path.join(out_dir, f'part_{k:02}.rs'), 'w', encoding='utf-8').write(chr(10).join(body))
    lib = ['//! NFS4.EXE recompiled to Rust (generated by tools/recomp.py). Do not edit by hand.',
           '#![allow(clippy::all)]',
           'use ps1rt::Cpu;']
    for k in range(parts_n):
        lib.append(f'mod part_{k:02};')
        lib.append(f'pub use part_{k:02}::*;')
    lib.append('/// Functions replaced on the host side (excluded from recompilation).')
    lib.append('pub const HLE: &[u32] = &[' + ', '.join(f'{x:#x}' for x in sorted(HLE_SET)) + '];')
    lib.append('/// Extents of all recompiled functions: (start, end, entry-at-pc).')
    lib.append('pub static RANGES: &[(u32, u32, fn(&mut Cpu, u32))] = &[')
    for f in funcs:
        lib.append(f'    ({f:#x}, {func_end(f):#x}, f_{f:08x}_at),')
    lib.append('];')
    lib.append('/// Continue execution at an arbitrary address inside a recompiled function.')
    lib.append('pub fn dispatch_at(c: &mut Cpu, pc: u32) -> bool {')
    lib.append('    // innermost (latest-starting) function whose extent contains pc')
    lib.append('    let i = RANGES.partition_point(|r| r.0 <= pc);')
    lib.append('    for r in RANGES[..i].iter().rev().take(8) {')
    lib.append('        if pc < r.1 { (r.2)(c, pc); return true; }')
    lib.append('    }')
    lib.append('    false')
    lib.append('}')
    lib.append('/// Dispatch a call by address into recompiled code.')
    lib.append('pub fn dispatch(c: &mut Cpu, addr: u32) -> bool {')
    lib.append('    match addr {')
    for f in funcs:
        lib.append(f'        {f:#x} => f_{f:08x}(c),')
    lib.append('        _ => return false,')
    lib.append('    }')
    lib.append('    true')
    lib.append('}')
    open(os.path.join(out_dir, 'lib.rs'), 'w', encoding='utf-8').write(chr(10).join(lib))
    print(f'{len(funcs)} functions -> {out_dir} ({parts_n} parts)')
