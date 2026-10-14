"""DuckStation automation: GDB remote protocol client + keyboard input to the emulator window.

Port-1 keyboard bindings added for automation (see docs): U/N/B/M = d-pad up/down/left/right,
Z = cross, X = circle, C = square, V = triangle, Y = start, O = select, 4/5/6/7 = L1/R1/L2/R2,
F10 = screenshot, F1/F2 = load/save state slot.
"""
import ctypes, ctypes.wintypes as wt, socket, struct, time

# ---------------------------------------------------------------- GDB remote

class Gdb:
    def __init__(self, host='127.0.0.1', port=2345, timeout=5.0):
        self.s = socket.create_connection((host, port), timeout=timeout)
        self.s.settimeout(timeout)
        self.buf = b''

    def _send(self, payload: str):
        cs = sum(payload.encode()) & 0xff
        self.s.sendall(f'${payload}#{cs:02x}'.encode())

    def _recv_packet(self, timeout=None):
        if timeout is not None:
            self.s.settimeout(timeout)
        while True:
            i = self.buf.find(b'$')
            j = self.buf.find(b'#', i + 1) if i >= 0 else -1
            if i >= 0 and j >= 0 and len(self.buf) >= j + 3:
                pkt = self.buf[i + 1:j].decode('latin1')
                self.buf = self.buf[j + 3:]
                self.s.sendall(b'+')
                return pkt
            chunk = self.s.recv(65536)
            if not chunk:
                raise ConnectionError('gdb closed')
            self.buf += chunk

    def cmd(self, payload: str, timeout=None):
        self._send(payload)
        return self._recv_packet(timeout)

    def read(self, addr: int, n: int) -> bytes:
        out = b''
        while n > 0:
            k = min(n, 0x800)
            r = self.cmd(f'm{addr:x},{k:x}')
            if r.startswith('E') or len(r) == 0:
                raise IOError(f'read {addr:#x}: {r}')
            out += bytes.fromhex(r)
            addr += k
            n -= k
        return out

    def write(self, addr: int, data: bytes):
        return self.cmd(f'M{addr:x},{len(data):x}:{data.hex()}')

    def regs(self):
        r = bytes.fromhex(self.cmd('g'))
        return list(struct.unpack_from('<%dI' % (len(r) // 4), r))

    def u32(self, a):
        return struct.unpack('<I', self.read(a, 4))[0]

    def interrupt(self):
        self.s.sendall(b'\x03')
        return self._recv_packet()

    def cont(self):
        self._send('c')

    def wait_stop(self, timeout=None):
        return self._recv_packet(timeout)

# ---------------------------------------------------------------- input

user32 = ctypes.WinDLL('user32', use_last_error=True)
INPUT_KEYBOARD, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE = 1, 2, 8

class KEYBDINPUT(ctypes.Structure):
    _fields_ = [('wVk', wt.WORD), ('wScan', wt.WORD), ('dwFlags', wt.DWORD), ('time', wt.DWORD), ('dwExtraInfo', ctypes.c_size_t)]

class INPUT(ctypes.Structure):
    class _U(ctypes.Union):
        _fields_ = [('ki', KEYBDINPUT), ('pad', ctypes.c_byte * 32)]
    _anonymous_ = ('u',)
    _fields_ = [('type', wt.DWORD), ('u', _U)]

VK = {c: ord(c) for c in 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'}
VK.update({'F1': 0x70, 'F2': 0x71, 'F3': 0x72, 'F4': 0x73, 'F10': 0x79, 'ESC': 0x1B, 'ENTER': 0x0D, 'TAB': 0x09})

def _key(vk, up):
    scan = user32.MapVirtualKeyW(vk, 0)
    i = INPUT(type=INPUT_KEYBOARD)
    i.ki = KEYBDINPUT(vk, scan, KEYEVENTF_KEYUP if up else 0, 0, 0)
    if user32.SendInput(1, ctypes.byref(i), ctypes.sizeof(INPUT)) != 1:
        # (the session is locked or input is blocked: keys would be lost silently)
        raise RuntimeError(f'SendInput refused (error {ctypes.get_last_error()}): is the session locked?')

def find_window(title_part='DuckStation'):
    found = []
    @ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
    def cb(h, _):
        n = user32.GetWindowTextLengthW(h)
        if n and user32.IsWindowVisible(h):
            b = ctypes.create_unicode_buffer(n + 1)
            user32.GetWindowTextW(h, b, n + 1)
            if title_part.lower() in b.value.lower():
                found.append((h, b.value))
        return True
    user32.EnumWindows(cb, 0)
    return found

def focus(title_part='Need for Speed'):
    w = find_window(title_part) or find_window('DuckStation')
    if not w:
        raise RuntimeError('DuckStation window not found')
    global TARGET_PID
    TARGET_PID = pid
    h = w[0][0]
    user32.ShowWindow(h, 9)
    user32.SetForegroundWindow(h)
    time.sleep(0.15)
    return h

def windows_of(pid):
    """Visible top-level windows of a process: [(hwnd, title)]."""
    found = []
    @ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
    def cb(h, _):
        p = wt.DWORD()
        user32.GetWindowThreadProcessId(h, ctypes.byref(p))
        n = user32.GetWindowTextLengthW(h)
        if p.value == pid and n and user32.IsWindowVisible(h):
            b = ctypes.create_unicode_buffer(n + 1)
            user32.GetWindowTextW(h, b, n + 1)
            found.append((h, b.value))
        return True
    user32.EnumWindows(cb, 0)
    return found

def focus_pid(pid):
    """Bring the process's main window to the foreground (Alt tap lets SetForegroundWindow work)."""
    w = [x for x in windows_of(pid) if 'need for speed' in x[1].lower()] or windows_of(pid)
    if not w:
        raise RuntimeError(f'no window for pid {pid}')
    h = w[0][0]
    user32.ShowWindow(h, 9)
    if user32.GetForegroundWindow() != h:
        # an Alt tap lets another process take the foreground; never send it to DuckStation
        # itself, where it would open the menu bar
        _key(0x12, False)
        _key(0x12, True)
        user32.SetForegroundWindow(h)
    time.sleep(0.3)
    return h

# the process keys are meant for (set by focus_pid): keys are only sent while its window is
# in front, never to whatever else the user has focused
TARGET_PID = None


def _foreground_pid():
    pid = ctypes.c_ulong(0)
    user32.GetWindowThreadProcessId(user32.GetForegroundWindow(), ctypes.byref(pid))
    return pid.value


def _ensure_target():
    if TARGET_PID is None or _foreground_pid() == TARGET_PID:
        return
    focus_pid(TARGET_PID)
    if _foreground_pid() != TARGET_PID:
        raise RuntimeError('DuckStation is not the foreground window: not sending keys')


def down(*keys):
    _ensure_target()
    for k in keys:
        _key(VK[k], False)

def up(*keys):
    for k in keys:
        _key(VK[k], True)

def tap(*keys, hold=0.08, gap=0.25):
    for k in keys:
        down(k)
        time.sleep(hold)
        up(k)
        time.sleep(gap)
