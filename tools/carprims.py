"""carprims.py RAM.bin [car]: the GPU packets in a car's ordering table (car+0x86C, length car+0x870)
from a RAM dump: what the PS1 draws of the car that frame (player car by default)."""
import struct, sys
r = open(sys.argv[1], 'rb').read()
R = lambda a: struct.unpack_from('<I', r, a & 0x1FFFFF)[0]
H = lambda a: struct.unpack_from('<h', r, a & 0x1FFFFF)[0]
car = int(sys.argv[2], 16) if len(sys.argv) > 2 else R(0x80110D0C)
ot, n = R(car + 0x86C), R(car + 0x870)
print(f'car {car:#x} ot {ot:#x} entries {n}')
lo, hi = ot & 0x1FFFFF, (ot & 0x1FFFFF) + 4 * max(n, 0)
seen = 0
for i in range(max(n, 0)):
    a = (ot & 0x1FFFFF) + 4 * i
    tag = R(0x80000000 | a)
    for _ in range(2000):
        nxt = tag & 0xFFFFFF
        if nxt == 0xFFFFFF or lo <= nxt < hi:
            break
        p = nxt
        tag = R(0x80000000 | p)
        ln = tag >> 24
        if ln == 0:
            continue
        code = r[p + 7]
        rgb = r[p + 4:p + 7].hex()
        k = code & 0xFC
        pts = []
        if k in (0x20, 0x28):  # flat poly
            pts = [struct.unpack_from('<hh', r, p + 8 + 4 * j) for j in range(3 if k == 0x20 else 4)]
        elif k in (0x30, 0x38):  # gouraud
            pts = [struct.unpack_from('<hh', r, p + 8 + 8 * j) for j in range(3 if k == 0x30 else 4)]
        elif k in (0x24, 0x2C):
            pts = [struct.unpack_from('<hh', r, p + 8 + 8 * j) for j in range(3 if k == 0x24 else 4)]
        elif k in (0x34, 0x3C):
            pts = [struct.unpack_from('<hh', r, p + 8 + 12 * j) for j in range(3 if k == 0x34 else 4)]
        extra = ''
        if k in (0x24, 0x2C, 0x34, 0x3C):
            tp = struct.unpack_from('<H', r, p + (22 if k in (0x24, 0x2C) else 26))[0]
            clut = struct.unpack_from('<H', r, p + 14)[0]
            uv = r[p + 12:p + 14].hex()
            extra = f' tpage={tp:04x} clut={clut:04x} uv0={uv}'
        cols = ''
        if k in (0x30, 0x38, 0x34, 0x3C):
            st = 8 if k in (0x30, 0x38) else 12
            cols = ' cols=' + ','.join(r[p + 4 + st * j:p + 7 + st * j].hex() for j in range(3 if k in (0x30, 0x34) else 4))
        print(f'ot{i:3d} {p:#08x} code {code:02x} len {ln} rgb {rgb}{cols} pts {pts}{extra}')
        seen += 1
print('packets', seen)
