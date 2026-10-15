"""Summarise memory dependencies of a snapshotted function (via snapemu)."""
import snapemu, sys, collections
for path in sys.argv[1:]:
    print('=====', path)
    snap, mem = snapemu.main(path)
    car = 0x801C19A8
    def bucket(x):
        if car <= x < car + 0x8F0: return 'car'
        if 0x801C19A8 <= x < 0x801C19A8 + 7 * 0x8F0: return 'othercars'
        if 0x80010000 <= x < 0x8013F800: return 'exe'
        if (x & 0x1FFFFFFF) >= 0x1F800000: return 'scratch'
        return 'heap:%05x' % ((x & 0x1FFFFF) >> 12)
    lb = collections.Counter(bucket(x) for x in mem.loads)
    sb = collections.Counter(bucket(x) for x in mem.stores)
    print('loads by region :', dict(sorted(lb.items())))
    print('stores by region:', dict(sorted(sb.items())))
