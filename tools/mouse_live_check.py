"""Does mouse movement reach the app while its settings window is open and NOT in front?

Starts the real app (window on, no focus taken), waits, makes small mouse movements that cancel
out, closes the window and reads the app's own count of the mouse reports it saw.

usage: python mouse_live_check.py <exe> [extra args...]
"""
import ctypes
import ctypes.wintypes as wt
import subprocess
import sys
import time

TITLE = "PS5 Remote Sim"
u32 = ctypes.windll.user32
u32.FindWindowW.restype = wt.HWND
u32.PostMessageW.argtypes = [wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM]


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [("dx", wt.LONG), ("dy", wt.LONG), ("mouseData", wt.DWORD), ("dwFlags", wt.DWORD),
                ("time", wt.DWORD), ("dwExtraInfo", ctypes.c_size_t)]


class INPUT(ctypes.Structure):
    _fields_ = [("type", wt.DWORD), ("mi", MOUSEINPUT)]


def move(dx, dy):
    i = INPUT(0, MOUSEINPUT(dx, dy, 0, 0x0001, 0, 0))          # relative move
    return u32.SendInput(1, ctypes.byref(i), ctypes.sizeof(INPUT))


def window_of(pid):
    """The app window that belongs to this process, never another copy's."""
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
    exe, extra = sys.argv[1], sys.argv[2:]
    assert ctypes.sizeof(INPUT) == 40, ctypes.sizeof(INPUT)
    proc = subprocess.Popen([exe, "--no-focus", "--count-mouse", *extra], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    deadline = time.time() + 20
    while not window_of(proc.pid) and proc.poll() is None and time.time() < deadline:
        time.sleep(0.1)
    hwnd = window_of(proc.pid)
    if not hwnd:
        print("FAILED: no window"); proc.kill(); return 1
    in_front = u32.GetForegroundWindow() == hwnd
    time.sleep(1.5)                                             # the window library has claimed the mouse by now
    sent = 0
    for _ in range(100):
        sent += move(2, 0); time.sleep(0.01)
        sent += move(-2, 0); time.sleep(0.01)
    time.sleep(0.3)
    u32.PostMessageW(hwnd, 0x0010, 0, 0)
    try:
        proc.wait(timeout=8)
    except subprocess.TimeoutExpired:
        proc.kill(); print("FAILED: did not close")
        return 1
    print("app window was in front:", in_front)
    print("movements made by this check:", sent)
    print(proc.stdout.read().strip() or "(the app printed nothing)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
