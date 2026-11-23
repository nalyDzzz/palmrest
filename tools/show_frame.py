"""Show saved game pictures in a borderless window and run one of the app's test aids against it.

This is how picture reading is checked without the game: the app copies this window the same way
it copies the game window.

usage: python tools/show_frame.py <aid> <picture.png> [more pictures...]
  <aid> is a test aid of the app that takes a window handle, e.g. --prompt-test or --overlay-test

The window is opened on a monitor other than the main one if there is one. It keeps answering
Windows while the app runs: a window that does not is never copied, and the app would hang on it.
Needs pygame.
"""
import ctypes
import ctypes.wintypes as wt
import os
import subprocess
import sys
import time

EXE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "target", "debug", "palmrest.exe")


def side_monitor():
    u32 = ctypes.windll.user32
    u32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    found = []

    class Info(ctypes.Structure):
        _fields_ = [("size", wt.DWORD), ("monitor", wt.RECT), ("work", wt.RECT), ("flags", wt.DWORD)]

    @ctypes.WINFUNCTYPE(ctypes.c_int, wt.HMONITOR, wt.HDC, ctypes.POINTER(wt.RECT), wt.LPARAM)
    def each(handle, _dc, _rect, _data):
        info = Info(ctypes.sizeof(Info))
        u32.GetMonitorInfoW(handle, ctypes.byref(info))
        m = info.monitor
        found.append(((m.left, m.top, m.right, m.bottom), bool(info.flags & 1)))
        return 1

    u32.EnumDisplayMonitors(None, None, each, 0)
    return ([m for m in found if not m[1]] or found)[0][0]


def main():
    aid, pictures = sys.argv[1], sys.argv[2:]
    left, top, right, bottom = side_monitor()
    width, height = min(1920, right - left), min(1080, bottom - top)
    os.environ["SDL_VIDEO_WINDOW_POS"] = f"{left},{top}"
    os.environ["PYGAME_HIDE_SUPPORT_PROMPT"] = "1"
    import pygame

    pygame.display.init()
    screen = pygame.display.set_mode((width, height), pygame.NOFRAME)
    hwnd = pygame.display.get_wm_info()["window"]

    def pump(seconds):
        end = time.time() + seconds
        while time.time() < end:
            pygame.event.pump()
            time.sleep(0.01)

    for path in pictures:
        picture = pygame.image.load(path)
        if picture.get_size() != (width, height):
            picture = pygame.transform.smoothscale(picture, (width, height))
        screen.blit(picture, (0, 0))
        pygame.display.flip()
        pump(0.3)
        app = subprocess.Popen([EXE, aid, str(hwnd)], stdout=subprocess.PIPE, text=True)
        started = time.time()
        while app.poll() is None and time.time() - started < 12:
            pump(0.05)
        if app.poll() is None:
            app.kill()
            print(os.path.basename(path), "-> the app did not finish")
            continue
        lines = [line for line in app.stdout.read().strip().splitlines() if line]
        print(os.path.basename(path), "->", lines[-2] if len(lines) > 1 else (lines[-1] if lines else "nothing"))
    pygame.display.quit()


if __name__ == "__main__":
    main()
