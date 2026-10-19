"""Transfer global variable names from the 1999 development build to the retail NFS4.EXE.

The functions are already matched (re/retail_syms.tsv, from symmatch.py). For each matched
pair the data addresses both versions reference (gp-relative, and lui + low half) are lined up
along the two instruction streams; each pairing is a vote for "dev global D is retail address
R". Globals no function references get the shift of their neighbours in the dev data when the
nearest matched symbols on both sides agree.

Writes re/retail_globals.tsv: retail address, name, dev address, how (ref = referenced by
code, near = by its neighbours), votes, type (from NFS4.SYM).

usage: globmatch.py [OUT.tsv]
"""
import collections
import difflib
import re
import struct
import sys
sys.path.insert(0, '.')
import mips
import snsym

OUT = sys.argv[1] if len(sys.argv) > 1 else '../re/retail_globals.tsv'
RETAIL_GP = 0x8013D83C

retail = mips.Exe('../gamedata/NFS4.EXE')
_front = open('../gamedata/FRONT.BIN', 'rb').read()
_img = bytearray(retail.data)
_img[0:len(_front)] = _front
retail.data = bytes(_img)

cpe = open('../NFS4.CPE', 'rb').read()
dev_mem = bytearray(0x130000)
o = 6
while o < len(cpe):
    t = cpe[o]; o += 1
    if t == 0:
        break
    if t == 1:
        a, n = struct.unpack_from('<II', cpe, o); o += 8
        dev_mem[a - 0x80010000:a - 0x80010000 + n] = cpe[o:o + n]; o += n
    elif t == 3:
        o += 6
    elif t == 8:
        o += 1
    else:
        raise SystemExit(f'unknown CPE chunk {t}')


def dev_word(a):
    return struct.unpack_from('<I', dev_mem, a - 0x80010000)[0]


recs = snsym.parse('../NFS4.SYM')
DEV_GP = next(r[1] for r in recs if r[0] == 'sym' and r[3] == '__SN_GP_BASE')
TEXT_LO, TEXT_HI = 0x8005797C, 0x8010CCD4

# dev globals: names and types (the SYM's external/static definitions outside the code)
dev_syms = {}
for r in recs:
    if r[0] == 'sym' and not TEXT_LO <= r[1] < TEXT_HI and r[1] >= 0x80010000 and not r[3].startswith('_'):
        dev_syms.setdefault(r[1], r[3])
dev_types = {}
for r in recs:
    if r[0] == 'def' and r[2] in (2, 3) and r[1] in dev_syms:
        _, val, cls, typ, size, dims, tag, name = r
        if (typ >> 4) & 3 != 2:  # not a function
            dev_types.setdefault(val, snsym.type_str(typ, tag, dims))

pairs = []
for line in open('../re/retail_syms.tsv'):
    r, n, d = line.rstrip('\n').split('\t')
    pairs.append((int(r, 16), int(d, 16), n))
r_starts = sorted(set(p[0] for p in pairs))
d_starts = sorted(set(p[1] for p in pairs))


def extent(starts, a, limit):
    import bisect
    i = bisect.bisect_right(starts, a)
    return min(starts[i] if i < len(starts) else limit, a + 0x4000)


def refs(word, lo, hi, gp):
    """Per instruction: (shape, data address or None)."""
    out = []
    lui = {}
    for pc in range(lo, hi, 4):
        w = word(pc)
        op, rs, rt = w >> 26, (w >> 21) & 31, (w >> 16) & 31
        imm = w & 0xFFFF
        s = imm - 0x10000 if imm & 0x8000 else imm
        shape = w >> 26 if op else 0x40 | (w & 63)
        addr = None
        if op == 15:
            lui[rt] = imm << 16
        elif op in (9, 13, 32, 33, 34, 35, 36, 37, 38, 40, 41, 42, 43, 46, 50, 58):
            if rs == 28:
                addr = (gp + s) & 0xFFFFFFFF
            elif rs in lui:
                addr = (lui[rs] + (imm if op == 13 else s)) & 0xFFFFFFFF
            if op in (9, 13) or op in (32, 33, 34, 35, 36, 37, 38):
                lui.pop(rt, None)
        elif op == 0 and (w & 63) not in (8, 9, 12, 13, 24, 25, 26, 27):
            lui.pop((w >> 11) & 31, None)
        elif op in (2, 3, 4, 5, 6, 7, 1) or (op == 0 and (w & 63) in (8, 9)):
            if op == 3 or (op == 0 and (w & 63) == 9):
                lui = {}
        else:
            lui.pop(rt, None)
        if addr is not None and not (0x80010000 <= addr < 0x80200000):
            addr = None
        out.append((shape, addr))
    return out


votes = collections.defaultdict(collections.Counter)
for r, d, name in pairs:
    if not retail.valid(r):
        continue
    rr = refs(retail.word, r, extent(r_starts, r, retail.end), RETAIL_GP)
    dr = refs(dev_word, d, extent(d_starts, d, TEXT_HI), DEV_GP)
    sm = difflib.SequenceMatcher(None, [x[0] for x in dr], [x[0] for x in rr], autojunk=False)
    for blk in sm.get_matching_blocks():
        for k in range(blk.size):
            da, ra = dr[blk.a + k][1], rr[blk.b + k][1]
            if da is not None and ra is not None:
                votes[da][ra] += 1

# a reference into a global (a field, an element) votes for the global holding it
dev_sorted = sorted(dev_syms)
import bisect
base_votes = collections.defaultdict(collections.Counter)
for da, c in votes.items():
    i = bisect.bisect_right(dev_sorted, da) - 1
    if i < 0:
        continue
    base = dev_sorted[i]
    for ra, n in c.items():
        base_votes[base][ra - (da - base)] += n

found = {}
for base, c in base_votes.items():
    (ra, n), = c.most_common(1)
    total = sum(c.values())
    if n * 2 > total:
        found[base] = (ra, 'ref', n)

# neighbours: the same shift on both sides
matched = sorted(found)
for d in dev_sorted:
    if d in found:
        continue
    i = bisect.bisect_left(matched, d)
    if 0 < i < len(matched):
        a, b = matched[i - 1], matched[i]
        sa, sb = found[a][0] - a, found[b][0] - b
        if sa == sb:
            found[d] = (d + sa, 'near', 0)

with open(OUT, 'w', encoding='utf-8', newline='\n') as f:
    for d in sorted(found, key=lambda d: found[d][0]):
        ra, how, n = found[d]
        f.write(f'{ra:08x}\t{dev_syms[d]}\t{d:08x}\t{how}\t{n}\t{dev_types.get(d, "")}\n')
print(f'{len(found)} of {len(dev_syms)} dev globals -> {OUT} ({sum(1 for v in found.values() if v[1] == "ref")} referenced)')
