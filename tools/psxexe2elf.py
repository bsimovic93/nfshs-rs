"""Wrap a PS-X EXE in a minimal ELF32 MIPS LE so disassemblers map it at its load address."""
import struct, sys
src, dst = sys.argv[1], sys.argv[2]
d = open(src, 'rb').read()
assert d[:8] == b'PS-X EXE'
pc0, gp0, taddr, tsize = struct.unpack_from('<IIII', d, 0x10)
text = d[0x800:0x800 + tsize]
ehsize, phsize = 52, 32
off = 0x1000
eh = b'\x7fELF' + bytes([1, 1, 1, 0]) + b'\0' * 8
eh += struct.pack('<HHIIIIIHHHHHH', 2, 8, 1, pc0, ehsize, 0, 0x1000, ehsize, phsize, 1, 40, 0, 0)
ph = struct.pack('<IIIIIIII', 1, off, taddr, taddr, len(text), len(text) + 0x100000, 7, 0x1000)
out = eh + ph
out += b'\0' * (off - len(out)) + text
open(dst, 'wb').write(out)
print(f'pc0={pc0:#x} gp={gp0:#x} taddr={taddr:#x} size={tsize:#x}')
