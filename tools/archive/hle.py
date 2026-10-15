"""Host implementations (HLE) of NFS4.EXE system-level functions for the snapshot emulator."""
import os, struct
import snapemu

DATA = os.path.join(os.path.dirname(__file__), '..', 'gamedata')
handles = {}
next_handle = [0x100]
file_log = []


def cstr(mem, a):
    out = bytearray()
    while True:
        b = mem.load(a, 1)
        if b == 0 or len(out) > 200:
            return out.decode('latin1')
        out.append(b)
        a += 1


def host_path(name):
    n = name.replace('cdrom:', '').replace('\\', '').lstrip('\\').split(';')[0]
    p = os.path.join(DATA, n.upper())
    return p if os.path.isfile(p) else None


def f_open(r, mem):   # 0x800EB204 open(name, mode, prio, &handle) -> 1/0
    name = cstr(mem, r[4])
    p = host_path(name)
    file_log.append(('open', name, bool(p)))
    if p:
        h = next_handle[0]; next_handle[0] += 1
        handles[h] = open(p, 'rb').read()
        mem.store(r[7], 4, h)
        r[2] = 1
    else:
        mem.store(r[7], 4, 0)
        r[2] = 0


def f_read(r, mem):   # 0x800EB27C read(handle, offset, buf, len) -> count
    d = handles.get(r[4], b'')
    off, buf, n = r[5], r[6], r[7]
    chunk = d[off:off + min(n, 0x7FFFFFFF)]
    for i, b in enumerate(chunk):
        mem.store(buf + i, 1, b)
    file_log.append(('read', r[4], off, hex(buf), len(chunk)))
    r[2] = len(chunk)


def f_close(r, mem):  # 0x800EB2AC close(handle, prio)
    handles.pop(r[4], None)
    r[2] = 0


def f_size(r, mem):   # 0x800EB300 size(handle, prio) -> bytes
    r[2] = len(handles.get(r[4], b''))


def install():
    snapemu.HLE.update({0x800EB204: f_open, 0x800EB27C: f_read, 0x800EB2AC: f_close, 0x800EB300: f_size})


def _rd(mem, a):
    return mem.load(a, 1)

def _wr(mem, a, v):
    mem.store(a, 1, v)

def _strlen(mem, a):
    n = 0
    while _rd(mem, a + n): n += 1
    return n

def bios_a0(r, mem, fn):
    """BIOS A0 table (string/memory subset). Returns True if handled."""
    a0, a1, a2 = r[4], r[5], r[6]
    if fn == 0x15:   # strcat
        d = a0 + _strlen(mem, a0); i = 0
        while True:
            c = _rd(mem, a1 + i); _wr(mem, d + i, c); i += 1
            if not c: break
        r[2] = a0
    elif fn == 0x16:  # strncat
        d = a0 + _strlen(mem, a0); i = 0
        while i < a2:
            c = _rd(mem, a1 + i)
            if not c: break
            _wr(mem, d + i, c); i += 1
        _wr(mem, d + i, 0); r[2] = a0
    elif fn in (0x17, 0x18):  # strcmp / strncmp
        i = 0; res = 0
        while fn == 0x17 or i < a2:
            x, y = _rd(mem, a0 + i), _rd(mem, a1 + i)
            if x != y: res = x - y; break
            if not x: break
            i += 1
        r[2] = res & 0xFFFFFFFF
    elif fn == 0x19:  # strcpy
        i = 0
        while True:
            c = _rd(mem, a1 + i); _wr(mem, a0 + i, c); i += 1
            if not c: break
        r[2] = a0
    elif fn == 0x1A:  # strncpy
        i = 0; end = False
        while i < a2:
            c = 0 if end else _rd(mem, a1 + i)
            if not c: end = True
            _wr(mem, a0 + i, c); i += 1
        r[2] = a0
    elif fn == 0x1B:  # strlen
        r[2] = _strlen(mem, a0)
    elif fn in (0x1C, 0x1E):  # index / strchr
        i = 0; res = 0
        while True:
            c = _rd(mem, a0 + i)
            if c == (a1 & 0xFF): res = a0 + i; break
            if not c: break
            i += 1
        r[2] = res
    elif fn in (0x1D, 0x1F):  # rindex / strrchr
        res = 0; i = 0
        while True:
            c = _rd(mem, a0 + i)
            if c == (a1 & 0xFF): res = a0 + i
            if not c: break
            i += 1
        r[2] = res
    elif fn == 0x25:  # toupper
        c = a0 & 0xFF; r[2] = c - 32 if 97 <= c <= 122 else c
    elif fn == 0x26:  # tolower
        c = a0 & 0xFF; r[2] = c + 32 if 65 <= c <= 90 else c
    elif fn in (0x2A, 0x2C):  # memcpy / memmove
        data = [_rd(mem, a1 + i) for i in range(a2)]
        for i, b in enumerate(data): _wr(mem, a0 + i, b)
        r[2] = a0
    elif fn == 0x2B:  # memset
        for i in range(a2): _wr(mem, a0 + i, a1 & 0xFF)
        r[2] = a0
    elif fn == 0x2D:  # memcmp
        res = 0
        for i in range(a2):
            x, y = _rd(mem, a0 + i), _rd(mem, a1 + i)
            if x != y: res = x - y; break
        r[2] = res & 0xFFFFFFFF
    elif fn == 0x2E:  # memchr
        res = 0
        for i in range(a2):
            if _rd(mem, a0 + i) == (a1 & 0xFF): res = a0 + i; break
        r[2] = res
    else:
        return False
    return True

snapemu.BIOS = bios_a0
