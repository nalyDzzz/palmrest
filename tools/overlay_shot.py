"""Show a saved game picture on a side monitor, run the app's test aid on it, and save what the
screen shows there: the picture with the app's overlay on top.

This is how the overlay is looked at without the game. Unlike window_shot.py it copies the
screen, because the overlay is made of separate windows that sit above the game's.

usage: python tools/overlay_shot.py <picture.png> <out.png>
  AID=--overlay-test   the test aid to run (default --prompt-test)
  EXTRA="--layout drive"   more arguments for the test aid
  WAIT=5.8             seconds to wait before copying the screen (default 2.6)
  CROPS="name=x,y,w,h;..."   also save these parts, enlarged three times, as <out>_<name>.png
Needs pygame.
"""
import ctypes
import ctypes.wintypes as wt
import os
import subprocess
import sys
import time

import show_frame


def main():
    picture_path, out_path = sys.argv[1], sys.argv[2]
    left, top, right, bottom = show_frame.side_monitor()
    width, height = min(1920, right - left), min(1080, bottom - top)
    os.environ["SDL_VIDEO_WINDOW_POS"] = f"{left},{top}"
    os.environ["PYGAME_HIDE_SUPPORT_PROMPT"] = "1"
    import pygame

    pygame.display.init()
    screen = pygame.display.set_mode((width, height), pygame.NOFRAME)
    hwnd = pygame.display.get_wm_info()["window"]
    picture = pygame.image.load(picture_path)
    if picture.get_size() != (width, height):
        picture = pygame.transform.smoothscale(picture, (width, height))
    screen.blit(picture, (0, 0))
    pygame.display.flip()

    def pump(seconds):
        # The window must keep answering Windows, or the app cannot copy it.
        end = time.time() + seconds
        while time.time() < end:
            pygame.event.pump()
            time.sleep(0.01)

    pump(0.3)
    app = subprocess.Popen([show_frame.EXE, os.environ.get("AID", "--prompt-test"), str(hwnd), *os.environ.get("EXTRA", "").split()],
                           stdout=subprocess.PIPE, text=True, encoding="utf-8")
    pump(float(os.environ.get("WAIT", "2.6")))

    u32, gdi = ctypes.windll.user32, ctypes.windll.gdi32
    u32.GetDC.restype = wt.HDC
    u32.GetDC.argtypes = [wt.HWND]
    gdi.CreateCompatibleDC.restype = wt.HDC
    gdi.CreateCompatibleDC.argtypes = [wt.HDC]
    gdi.CreateCompatibleBitmap.restype = wt.HBITMAP
    gdi.CreateCompatibleBitmap.argtypes = [wt.HDC, ctypes.c_int, ctypes.c_int]
    gdi.SelectObject.restype = wt.HGDIOBJ
    gdi.SelectObject.argtypes = [wt.HDC, wt.HGDIOBJ]
    gdi.BitBlt.argtypes = [wt.HDC, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wt.HDC, ctypes.c_int, ctypes.c_int, wt.DWORD]
    gdi.GetDIBits.argtypes = [wt.HDC, wt.HBITMAP, wt.UINT, wt.UINT, ctypes.c_void_p, ctypes.c_void_p, wt.UINT]
    desktop = u32.GetDC(None)
    memory = gdi.CreateCompatibleDC(desktop)
    bitmap = gdi.CreateCompatibleBitmap(desktop, width, height)
    gdi.SelectObject(memory, bitmap)
    # SRCCOPY | CAPTUREBLT: layered windows (the overlay) are included.
    gdi.BitBlt(memory, 0, 0, width, height, desktop, left, top, 0x00CC0020 | 0x40000000)

    class Header(ctypes.Structure):
        _fields_ = [("size", wt.DWORD), ("width", wt.LONG), ("height", wt.LONG), ("planes", wt.WORD), ("bits", wt.WORD), ("compression", wt.DWORD),
                    ("image_size", wt.DWORD), ("x_per_meter", wt.LONG), ("y_per_meter", wt.LONG), ("used", wt.DWORD), ("important", wt.DWORD)]

    header = Header(ctypes.sizeof(Header), width, -height, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels = ctypes.create_string_buffer(width * height * 4)
    gdi.GetDIBits(memory, bitmap, 0, height, pixels, ctypes.byref(header), 0)
    shot = pygame.image.frombuffer(pixels.raw, (width, height), "BGRA")
    pygame.image.save(shot, out_path)
    for part in filter(None, os.environ.get("CROPS", "").split(";")):
        name, box = part.split("=")
        x, y, w, h = (int(v) for v in box.split(","))
        pygame.image.save(pygame.transform.scale(shot.subsurface(pygame.Rect(x, y, w, h)), (w * 3, h * 3)), out_path.replace(".png", f"_{name}.png"))

    started = time.time()
    while app.poll() is None and time.time() - started < 8:
        pump(0.05)
    if app.poll() is None:
        app.kill()
    lines = [line for line in app.stdout.read().strip().splitlines() if line]
    print(lines[-2] if len(lines) > 1 else lines)
    pygame.display.quit()


if __name__ == "__main__":
    main()
