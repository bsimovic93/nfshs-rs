"""Parser for SN Systems PSY-Q debug symbol files (.SYM, 'MND' magic).

usage: snsym.py NFS4.SYM struct NAME     dump a struct/union layout
       snsym.py NFS4.SYM structs > out   dump every struct
       snsym.py NFS4.SYM funcs           list functions (address, file, line, name)
"""
import struct, sys

BASE = {0: 'void', 1: 'void', 2: 'char', 3: 'short', 4: 'int', 5: 'long', 6: 'float', 7: 'double',
        8: 'struct', 9: 'union', 10: 'enum', 11: 'moe', 12: 'uchar', 13: 'ushort', 14: 'uint', 15: 'ulong'}


def type_str(t, tag, dims):
    base = BASE.get(t & 0xF, f'?{t & 0xF}')
    if base in ('struct', 'union', 'enum'):
        base = f'{base} {tag}'
    d = t >> 4
    parts = []
    while d:
        k = d & 3
        parts.append({1: '*', 2: '()', 3: '[]'}.get(k, ''))
        d >>= 2
    s = base
    di = 0
    for p in parts:
        if p == '[]':
            s += f'[{dims[di] if di < len(dims) else ""}]'
            di += 1
        else:
            s += p
    return s


def parse(path):
    d = open(path, 'rb').read()
    assert d[:3] == b'MND'
    o = 8
    recs = []
    while o + 5 <= len(d):
        val = struct.unpack_from('<I', d, o)[0]
        tag = d[o + 4]
        o += 5
        if tag < 0x80:
            n = d[o]; name = d[o + 1:o + 1 + n].decode('latin1'); o += 1 + n
            recs.append(('sym', val, tag, name))
        elif tag == 0x80:
            pass
        elif tag == 0x82:
            o += 1
        elif tag == 0x84:
            o += 2
        elif tag == 0x86:
            o += 4
        elif tag == 0x88:
            line = struct.unpack_from('<I', d, o)[0]; n = d[o + 4]
            name = d[o + 5:o + 5 + n].decode('latin1'); o += 5 + n
            recs.append(('file', val, line, name))
        elif tag == 0x8A:
            pass
        elif tag == 0x8C:
            fp, fsize, retreg, mask, maskoffs, line = struct.unpack_from('<HIHIiI', d, o)
            o += 2 + 4 + 2 + 4 + 4 + 4
            n = d[o]; fname = d[o + 1:o + 1 + n].decode('latin1'); o += 1 + n
            n = d[o]; name = d[o + 1:o + 1 + n].decode('latin1'); o += 1 + n
            recs.append(('func', val, line, fname, name, fsize))
        elif tag in (0x8E, 0x90, 0x92):
            o += 4
        elif tag == 0x94:
            cls, typ, size = struct.unpack_from('<HHI', d, o); o += 8
            n = d[o]; name = d[o + 1:o + 1 + n].decode('latin1'); o += 1 + n
            recs.append(('def', val, cls, typ, size, [], '', name))
        elif tag == 0x96:
            cls, typ, size, ndim = struct.unpack_from('<HHIH', d, o); o += 10
            dims = list(struct.unpack_from('<%dI' % ndim, d, o)); o += 4 * ndim
            n = d[o]; tagname = d[o + 1:o + 1 + n].decode('latin1'); o += 1 + n
            n = d[o]; name = d[o + 1:o + 1 + n].decode('latin1'); o += 1 + n
            recs.append(('def', val, cls, typ, size, dims, tagname, name))
        elif tag == 0x98:
            o += 8
        elif tag == 0x9A:
            pass
        else:
            raise ValueError(f'unknown tag {tag:#x} at {o - 5:#x}')
    return recs


def structs(recs):
    """Collect struct/union definitions: STRTAG(10)/UNTAG(12) followed by members until EOS(102)."""
    out = {}
    cur = None
    for r in recs:
        if r[0] != 'def':
            continue
        _, val, cls, typ, size, dims, tag, name = r
        if cls in (10, 12):
            cur = (name, size, [])
        elif cls in (8, 11) and cur is not None:
            cur[2].append((val, name, type_str(typ, tag, dims), size))
        elif cls == 102 and cur is not None:
            out.setdefault(cur[0], (cur[1], cur[2]))
            cur = None
    return out


if __name__ == '__main__':
    recs = parse(sys.argv[1])
    cmd = sys.argv[2]
    if cmd == 'struct':
        st = structs(recs)
        size, members = st[sys.argv[3]]
        print(f'struct {sys.argv[3]} /* size {size:#x} */ {{')
        for off, name, t, sz in members:
            print(f'    /* +{off:#05x} */ {t} {name};  // size {sz:#x}')
        print('};')
    elif cmd == 'structs':
        for n, (size, members) in sorted(structs(recs).items()):
            print(f'struct {n} /* size {size:#x} */ {{')
            for off, name, t, sz in members:
                print(f'    /* +{off:#05x} */ {t} {name};')
            print('};\n')
    elif cmd == 'funcs':
        for r in recs:
            if r[0] == 'func':
                print(f'{r[1]:08x}\t{r[3]}:{r[2]}\t{r[4]}\tframe={r[5]}')
