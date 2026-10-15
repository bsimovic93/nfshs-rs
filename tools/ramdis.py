"""ramdis.py RAM.bin ADDR COUNT: disassemble from a 2 MB RAM dump (overlays such as the front end)."""
import sys
sys.path.insert(0, '.')
import mips


class Ram(mips.Exe):
    def __init__(self, path):
        self.data = open(path, 'rb').read()
        self.base = 0x8000_0000
        self.end = self.base + len(self.data)
        self.pc0 = 0


if __name__ == '__main__':
    print('\n'.join(mips.annotate(Ram(sys.argv[1]), int(sys.argv[2], 16), int(sys.argv[3]))))
