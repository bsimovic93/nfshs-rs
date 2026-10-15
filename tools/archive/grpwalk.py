import struct, sys, collections
d = open(sys.argv[1], 'rb').read()
def walk(off, end, depth, out):
    while off + 16 <= end:
        t, sz, pad, cnt = struct.unpack_from('<IIII', d, off)
        out.append((depth, off, t, sz, pad, cnt))
        if sz < 16 or off + sz > end:
            out.append((depth, off, 'BAD', sz, pad, cnt)); return
        off += sz
out = []
t, sz, pad, cnt = struct.unpack_from('<IIII', d, 0)
print(f'root type={t:#x} size={sz} count={cnt}')
walk(16, sz, 1, out)
c = collections.Counter(o[2] for o in out)
print(c)
for o in out[:60]:
    print(o[0]*'  ', f'@{o[1]:#x} type={o[2] if isinstance(o[2],str) else hex(o[2])} size={o[3]:#x} pad={o[4]:#x} cnt={o[5]}')
print(len(out))
