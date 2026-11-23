"""Save a picture of one window (found by its title) to a PNG, without bringing it to the front.

Only that window's own content is captured, not the screen around it.

usage: python window_shot.py "<window title>" <out.png> [--close]
  --close   ask the window to close afterwards (the same as clicking its X)
"""
import ctypes
import ctypes.wintypes as wt
import os
import sys

os.environ["PYGAME_HIDE_SUPPORT_PROMPT"] = "1"
import pygame

u32, gdi = ctypes.windll.user32, ctypes.windll.gdi32
u32.FindWindowW.restype = wt.HWND
u32.GetDC.restype = wt.HDC; u32.GetDC.argtypes = [wt.HWND]
gdi.CreateCompatibleDC.restype = wt.HDC; gdi.CreateCompatibleDC.argtypes = [wt.HDC]
gdi.CreateCompatibleBitmap.restype = wt.HBITMAP; gdi.CreateCompatibleBitmap.argtypes = [wt.HDC, ctypes.c_int, ctypes.c_int]
gdi.SelectObject.restype = wt.HGDIOBJ; gdi.SelectObject.argtypes = [wt.HDC, wt.HGDIOBJ]
u32.PrintWindow.argtypes = [wt.HWND, wt.HDC, wt.UINT]
gdi.GetDIBits.argtypes = [wt.HDC, wt.HBITMAP, wt.UINT, wt.UINT, ctypes.c_void_p, ctypes.c_void_p, wt.UINT]
gdi.DeleteObject.argtypes = [wt.HGDIOBJ]; gdi.DeleteDC.argtypes = [wt.HDC]
u32.ReleaseDC.argtypes = [wt.HWND, wt.HDC]
u32.PostMessageW.argtypes = [wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM]
ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))


def main():
    if sys.argv[1] == "--hwnd":                      # a specific window, by its handle
        hwnd, out = int(sys.argv[2]), sys.argv[3]
    else:
        title, out = sys.argv[1], sys.argv[2]
        hwnd = u32.FindWindowW(None, title)
        if not hwnd:
            print("FAILED: no window titled", repr(title)); return 1
    r = wt.RECT(); u32.GetClientRect(hwnd, ctypes.byref(r))
    w, h = r.right, r.bottom
    wr = wt.RECT(); u32.GetWindowRect(hwnd, ctypes.byref(wr))
    print(f"window at ({wr.left},{wr.top}), content {w}x{h}")
    if w > 0 and h > 0:
        dc = u32.GetDC(hwnd); mdc = gdi.CreateCompatibleDC(dc); bmp = gdi.CreateCompatibleBitmap(dc, w, h)
        old = gdi.SelectObject(mdc, bmp)
        ok = u32.PrintWindow(hwnd, mdc, 3)          # client area only, full content
        hdr = (ctypes.c_uint32 * 10)(40, w, (-h) & 0xFFFFFFFF, 1 | (32 << 16), 0, 0, 0, 0, 0, 0)
        buf = ctypes.create_string_buffer(w * h * 4)
        gdi.GetDIBits(mdc, bmp, 0, h, buf, hdr, 0)
        gdi.SelectObject(mdc, old); gdi.DeleteObject(bmp); gdi.DeleteDC(mdc); u32.ReleaseDC(hwnd, dc)
        pygame.image.save(pygame.image.frombuffer(buf.raw, (w, h), "BGRA"), out)
        print("saved" if ok else "PrintWindow reported failure; saved what it gave", out)
    if "--close" in sys.argv:
        u32.PostMessageW(hwnd, 0x0010, 0, 0)        # WM_CLOSE
        print("asked the window to close")
    return 0


if __name__ == "__main__":
    sys.exit(main())
