import struct, sys
d = open(sys.argv[1], 'rb').read()
off = int(sys.argv[2], 16)
t, sz, pad, cnt = struct.unpack_from('<IIII', d, off)
print(f'block type={t:#x} size={sz:#x} cnt={cnt}')
o = off + 16
end = off + sz
while o + 16 <= end:
    t, s, p, c = struct.unpack_from('<IIII', d, o)
    print(f' @{o:#x} type={t:#x} size={s:#x} pad={p:#x} cnt={c}')
    body = d[o+16:o+min(s, 16+96)]
    for i in range(0, len(body), 32):
        print('    ', body[i:i+32].hex(' ', 4))
    if s < 16: break
    o += s
