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


def func_end(start):
    """End = next known function start or next prologue after the first instruction."""
    import bisect
    k = bisect.bisect_right(SORTED_STARTS, start)
    nxt = SORTED_STARTS[k] if k < len(SORTED_STARTS) else e.end
    p = start + 4
    while p < nxt and e.valid(p):
        if is_prologue(e.word(p)):
            # a prologue in the middle could also be an alloca; accept only if preceded by jr ra delay
            if e.word(p - 8) == 0x03E00008:
                return p
        p += 4
    return nxt


def calls_of(start):
    end = func_end(start)
    out = set()
    for q in range(start, end, 4):
        w = e.word(q)
        if w >> 26 == 3:
            out.add((q & 0xF0000000) | ((w & 0x3FFFFFF) << 2))
    return out


def collect(roots):
    seen = set()
    stack = list(roots)
    while stack:
        f = stack.pop()
        if f in seen:
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
    if has_indirect:
        leaders |= set(words)
    leaders = sorted(x for x in leaders if f <= x < end)
    out = [f'pub fn f_{f:08x}(c: &mut Cpu) {{', f'    let mut pc: u32 = {f:#x};', '    loop {', '        match pc {']
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
            delay_w = words.get(pc + 4, 0)
            delay = gen_simple(delay_w, pc + 4) or ''
            if delay is None:
                raise ValueError(f'branch in delay slot at {pc:#x}')
            dl = f'                {delay} // delay {pc + 4:#x}' if delay else ''
            comment = f' // {pc:#x}: {mips.dis1(w, pc)}'
            if kind == 'b':
                body.append(f'                let cond = {bi[1]};{comment}')
                if dl: body.append(dl)
                body.append(f'                if cond {{ pc = {bi[2]:#x}; continue; }}')
                body.append(f'                pc = {pc + 8:#x}; continue;')
            elif kind == 'bal':
                raise ValueError(f'bltzal/bgezal at {pc:#x}')
            elif kind == 'j':
                if dl: body.append(dl)
                if f <= bi[2] < end:
                    body.append(f'                pc = {bi[2]:#x}; continue;{comment}')
                else:  # tail call
                    body.append(f'                {call_expr(bi[2], known)} return;{comment}')
            elif kind == 'jal':
                if dl: body.append(dl)
                body.append(f'                c.r[31] = {pc + 8:#x};{comment}')
                body.append(f'                {call_expr(bi[2], known)}')
                body.append(f'                pc = {pc + 8:#x}; continue;')
            elif kind == 'jr':
                rs = bi[1]
                body.append(f'                let target = {reg(rs)};{comment}')
                if dl: body.append(dl)
                if rs == 31:
                    body.append('                let _ = target; return;')
                else:
                    body.append('                pc = target; continue;')
            elif kind == 'jalr':
                rs, rd = bi[1], bi[2]
                body.append(f'                let target = {reg(rs)};{comment}')
                if dl: body.append(dl)
                if rd: body.append(f'                c.r[{rd}] = {pc + 8:#x};')
                body.append('                c.call(target);')
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
    out.append(f'            _ => panic!("f_{f:08x}: bad pc {{pc:#x}}"),')
    out += ['        }', '    }', '}', '']
    return '\n'.join(out)


def call_expr(t, known):
    return f'f_{t:08x}(c);' if t in known else f'c.call({t:#x});'


if __name__ == '__main__':
    out_path = sys.argv[1]
    roots = [int(x, 16) for x in sys.argv[2:]]
    funcs = collect(roots)
    known = set(funcs)
    parts = ['//! Generated by tools/recomp.py from NFS4.EXE (NTSC-U). Do not edit by hand.',
             '//! Roots: ' + ', '.join(f'{r:#x}' for r in roots),
             '#![allow(unused_variables, unused_assignments, clippy::all, unreachable_code, non_snake_case)]',
             'use super::Cpu;', '']
    for f in funcs:
        parts.append(gen_function(f, known))
    parts.append('/// Dispatch for indirect calls into recompiled code.')
    parts.append('pub fn dispatch(c: &mut Cpu, addr: u32) -> bool {')
    parts.append('    match addr {')
    for f in funcs:
        parts.append(f'        {f:#x} => f_{f:08x}(c),')
    parts.append('        _ => return false,')
    parts.append('    }')
    parts.append('    true')
    parts.append('}')
    open(out_path, 'w', encoding='utf-8').write('\n'.join(parts))
    print(f'{len(funcs)} functions -> {out_path}')
