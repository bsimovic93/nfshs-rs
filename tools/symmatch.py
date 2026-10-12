"""Transfer symbol names from the 1999 development build (NFS4.CPE + NFS4.MAP) to the retail
NFS4.EXE by matching functions on address-independent instruction fingerprints, then
propagating matches through call graphs.

usage: symmatch.py OUT.tsv
"""
import re, struct, sys, collections, hashlib
sys.path.insert(0, '.')
import recomp, mips

retail = recomp.e  # retail NFS4.EXE

# ---- development build image from the CPE
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


# ---- dev symbols (functions = symbols inside .text)
syms = {}
for line in open('../NFS4.MAP'):
    m = re.match(r'\s+([0-9A-F]{8})\s+(\S+)\s*$', line)
    if m:
        syms.setdefault(int(m.group(1), 16), m.group(2))
TEXT_LO, TEXT_HI = 0x8005797C, 0x8010CCD4
dev_funcs = sorted(a for a in syms if TEXT_LO <= a < TEXT_HI)
dev_ext = {a: (dev_funcs[i + 1] if i + 1 < len(dev_funcs) else TEXT_HI) for i, a in enumerate(dev_funcs)}


def norm(w):
    op = w >> 26
    if op in (2, 3):
        return op << 26                     # jump targets differ between builds
    if op == 15:
        return w & 0xFFFF0000               # lui: address high half
    if op in (8, 9, 10, 11, 12, 13, 14) or op >= 32:
        rs = (w >> 21) & 31
        imm = w & 0xFFFF
        if rs == 28 or imm >= 0x1000 and imm < 0xF000:
            return w & 0xFFFF0000           # gp-relative / address low half
    return w


def fingerprint(words):
    h = hashlib.sha1()
    for w in words:
        h.update(struct.pack('<I', norm(w)))
    return h.hexdigest()


def jal_targets(words, base):
    out = []
    for i, w in enumerate(words):
        if w >> 26 == 3:
            out.append(((base + 4 * i) & 0xF0000000) | ((w & 0x3FFFFFF) << 2))
    return out


dev_words = {a: [dev_word(x) for x in range(a, dev_ext[a], 4)] for a in dev_funcs}
# trim trailing padding/nops after the final jr ra so sizes compare
def trim(ws):
    last = max((i for i, w in enumerate(ws) if w == 0x03E00008), default=len(ws) - 2)
    return ws[:last + 2]
dev_words = {a: trim(w) for a, w in dev_words.items() if w}

ret_funcs = recomp.all_starts()
ret_words = {a: [retail.word(x) for x in range(a, recomp.func_end(a), 4)] for a in ret_funcs}

dev_fp = collections.defaultdict(list)
for a, w in dev_words.items():
    dev_fp[(len(w), fingerprint(w))].append(a)
ret_fp = collections.defaultdict(list)
for a, w in ret_words.items():
    ret_fp[(len(w), fingerprint(w))].append(a)

match = {}  # retail -> dev
for k, ds in dev_fp.items():
    rs = ret_fp.get(k)
    if rs and len(ds) == 1 and len(rs) == 1:
        match[rs[0]] = ds[0]
print('unique fingerprint matches:', len(match))

# ---- propagate through call graphs: matched functions with equal call counts pair callees in order
for it in range(10):
    added = 0
    for r, d in list(match.items()):
        if d not in dev_words or r not in ret_words:
            continue
        rc = jal_targets(ret_words[r], r)
        dc = jal_targets(dev_words[d], d)
        if len(rc) != len(dc):
            continue
        for x, y in zip(rc, dc):
            if x not in match and y in dev_words and x in ret_words:
                match[x] = y
                added += 1
    print(f'propagation pass {it}: +{added}')
    if not added:
        break

# also data symbols referenced identically? (left for later)
with open(sys.argv[1], 'w') as f:
    for r in sorted(match):
        f.write(f'{r:08x}\t{syms[match[r]]}\t{match[r]:08x}\n')
print('total named retail functions:', len(match), 'of', len(ret_funcs))
