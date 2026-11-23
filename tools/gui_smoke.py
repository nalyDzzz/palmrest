"""Start the app without taking focus, save a picture of its window, close it, report how it exited.

usage: python gui_smoke.py <exe> <out.png> [extra args for the exe...]

Give the exe by its full path. Useful extra args: --dry (look only: no keyboard or mouse is read
and no controller is plugged in, so it is safe while a game is being played with another copy),
--at x,y, --group triggers, --select G, --layout drive.
"""
import ctypes
import ctypes.wintypes as wt
import subprocess
import sys
import time

TITLE = "Palmrest"
u32 = ctypes.windll.user32
u32.FindWindowW.restype = wt.HWND


def window_of(pid):
    found = []
    @ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
    def each(hwnd, _):
        owner = wt.DWORD(); u32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid:
            buf = ctypes.create_unicode_buffer(128); u32.GetWindowTextW(hwnd, buf, 128)
            if buf.value == TITLE:
                found.append(hwnd)
        return True
    u32.EnumWindows(each, 0)
    return found[0] if found else None


def main():
    exe, out, extra = sys.argv[1], sys.argv[2], sys.argv[3:]
    proc = subprocess.Popen([exe, "--no-focus", *extra], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    deadline = time.time() + 20
    while not window_of(proc.pid) and proc.poll() is None and time.time() < deadline:
        time.sleep(0.1)
    if proc.poll() is not None:
        print("FAILED: the app exited on its own, code", proc.returncode); print(proc.stderr.read()[-3000:]); return 1
    hwnd = window_of(proc.pid)
    if not hwnd:
        print("FAILED: no window appeared within 20 s"); proc.kill(); return 1
    time.sleep(2.0)                                   # let it draw a few frames
    shot = subprocess.run([sys.executable, "-X", "utf8", __file__.replace("gui_smoke.py", "window_shot.py"), "--hwnd", str(hwnd), out, "--close"],
                          capture_output=True, text=True)
    print(shot.stdout.strip()); print(shot.stderr.strip()[-800:])
    try:
        code = proc.wait(timeout=8)
        print("app closed, exit code", code)
    except subprocess.TimeoutExpired:
        print("FAILED: the app did not close within 8 s; stopping it"); proc.kill(); return 1
    err = proc.stderr.read().strip()
    if err:
        print("stderr:", err[-2000:])
    return 0


if __name__ == "__main__":
    sys.exit(main())
