import struct, sys
d = open(sys.argv[1], 'rb').read()
start = int(sys.argv[2], 16); maxdepth = int(sys.argv[3]) if len(sys.argv) > 3 else 9
nbytes = int(sys.argv[4]) if len(sys.argv) > 4 else 64
def is_chunk(o, end):
    if o + 16 > end: return False
    t, s, p, c = struct.unpack_from('<IIII', d, o)
    return p == 0xcdcdcdcd and 16 <= s <= end - o and t < 0x100
def show(o, end, depth):
    while is_chunk(o, end):
        t, s, p, c = struct.unpack_from('<IIII', d, o)
        print('  '*depth + f'@{o:#x} type={t:#x} size={s:#x} cnt={c}')
        if is_chunk(o+16, o+s) and depth < maxdepth:
            show(o+16, o+s, depth+1)
        else:
            body = d[o+16:o+min(s, 16+nbytes)]
            for i in range(0, len(body), 32):
                print('  '*depth + '    ' + body[i:i+32].hex(' ', 2))
        o += s
t, s, p, c = struct.unpack_from('<IIII', d, start)
show(start, start + s, 0)
