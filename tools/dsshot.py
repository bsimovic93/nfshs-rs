"""Capture the DuckStation game window to PNG (Win32 PrintWindow, no dependencies)."""
import ctypes, ctypes.wintypes as wt, struct, sys, zlib
import ds

gdi32 = ctypes.WinDLL('gdi32')
user32 = ds.user32

class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [('biSize', wt.DWORD), ('biWidth', wt.LONG), ('biHeight', wt.LONG), ('biPlanes', wt.WORD),
                ('biBitCount', wt.WORD), ('biCompression', wt.DWORD), ('biSizeImage', wt.DWORD),
                ('biXPelsPerMeter', wt.LONG), ('biYPelsPerMeter', wt.LONG), ('biClrUsed', wt.DWORD), ('biClrImportant', wt.DWORD)]

def png(path, w, h, rgb):
    raw = b''.join(b'\0' + rgb[y * w * 3:(y + 1) * w * 3] for y in range(h))
    def chunk(t, d):
        return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
    data = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw, 6)) + chunk(b'IEND', b'')
    open(path, 'wb').write(data)

def capture(hwnd):
    r = wt.RECT()
    user32.GetClientRect(hwnd, ctypes.byref(r))
    w, h = r.right, r.bottom
    hdc = user32.GetDC(hwnd)
    mdc = gdi32.CreateCompatibleDC(hdc)
    bmp = gdi32.CreateCompatibleBitmap(hdc, w, h)
    gdi32.SelectObject(mdc, bmp)
    user32.PrintWindow(hwnd, mdc, 3)  # PW_CLIENTONLY | PW_RENDERFULLCONTENT
    bi = BITMAPINFOHEADER(ctypes.sizeof(BITMAPINFOHEADER), w, -h, 1, 32, 0, 0, 0, 0, 0, 0)
    buf = ctypes.create_string_buffer(w * h * 4)
    gdi32.GetDIBits(mdc, bmp, 0, h, buf, ctypes.byref(bi), 0)
    gdi32.DeleteObject(bmp); gdi32.DeleteDC(mdc); user32.ReleaseDC(hwnd, hdc)
    b = buf.raw
    rgb = bytearray(w * h * 3)
    rgb[0::3] = b[2::4]; rgb[1::3] = b[1::4]; rgb[2::3] = b[0::4]
    return w, h, bytes(rgb)

def capture_to(hwnd, out):
    w, h, rgb = capture(hwnd)
    png(out, w, h, rgb)
    return out, w, h

def shot(out, title='Need for Speed - High Stakes'):
    wins = [x for x in ds.find_window(title) if x[1] == title] or ds.find_window(title)
    w, h, rgb = capture(wins[0][0])
    png(out, w, h, rgb)
    return out, w, h

if __name__ == '__main__':
    print(shot(sys.argv[1]))
