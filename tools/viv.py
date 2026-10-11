import struct, sys, os
def bigf(d):
    assert d[:4] == b'BIGF'
    n, hdr = struct.unpack_from('>II', d, 8)
    o = 16; out = []
    for _ in range(n):
        off, sz = struct.unpack_from('>II', d, o); o += 8
        e = d.index(b'\0', o); name = d[o:e].decode(); o = e + 1
        out.append((name, d[off:off+sz]))
    return out
if __name__ == '__main__':
    d = open(sys.argv[1], 'rb').read()
    od = sys.argv[2]; os.makedirs(od, exist_ok=True)
    for name, b in bigf(d):
        print(f'{len(b):8} {name} {b[:16].hex()}')
        open(os.path.join(od, name), 'wb').write(b)
