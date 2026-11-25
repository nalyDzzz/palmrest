"""Time how long the app takes to turn a key press, a mouse click and mouse movement into
controller output, as a Remote Play program reading the virtual controller would see it.

Starts `<exe> --latency-test python.exe`: the real engine on the default keys, taking over while
this script's window is in front, and taking presses made by a program as well. The script brings
its window to the front, presses Space (cross) and the left mouse button (R2) and moves the mouse
with SendInput, and reads the virtual controller through SDL (pygame) as fast as it can. Presses
are also split into: Windows (SendInput -> the app's hook), the app (hook -> report taken by the
driver), and the driver plus the reader (-> seen here). All times are on the high-resolution
counter, which both programs read.

Mouse movement is sent as 1000 reports a second, but the app does not get that many: Windows 11
bundles the mouse reports of a program in the background (since its July 2023 update) to about
125 a second while the mouse moves one way. The app is always in the background in play (the
Remote Play program is in front), so a real mouse is very likely bundled the same way there; this
was seen here with injected movement only. The rate that reached the app is reported.

Not while a Remote Play program (Veyra, chiaki) is open: it would pass the presses to the console.
Hands off the keyboard and mouse while it runs (about 30 seconds): real input would mix in. It
stops if its window loses the front.

usage: python tools/latency_bench.py <path to palmrest.exe> [--presses N] [--moves N] [--sensitivity S]
Use the release exe (`cargo build --release`): timings from the debug exe mean nothing.
"""
import argparse
import bisect
import ctypes
import ctypes.wintypes as wt
import os
import random
import statistics
import subprocess
import sys
import threading

os.environ["PYGAME_HIDE_SUPPORT_PROMPT"] = "1"
os.environ["SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS"] = "1"
import pygame
from pygame._sdl2 import controller

from show_frame import side_monitor

REMOTE_PLAY = {"veyra_qml_ui.exe", "chiaki-ng.exe", "chiaki.exe", "remoteplay.exe"}
# The aim settings the app starts with (config.rs, AimConfig): what a steady speed should tilt to.
FULL_SPEED_AT_1, DEADZONE, CURVE = 6000.0, 0.30, 1.8
MOVE_SHARES = (0.05, 0.15, 0.4, 0.8)
MOVE_FOR, WATCH_AFTER = 0.30, 0.30

u32, k32 = ctypes.windll.user32, ctypes.windll.kernel32
u32.GetForegroundWindow.restype = wt.HWND
u32.GetWindowThreadProcessId.argtypes = [wt.HWND, ctypes.c_void_p]
u32.AttachThreadInput.argtypes = [wt.DWORD, wt.DWORD, wt.BOOL]
u32.SetForegroundWindow.argtypes = [wt.HWND]
u32.BringWindowToTop.argtypes = [wt.HWND]
u32.ShowWindow.argtypes = [wt.HWND, ctypes.c_int]
u32.ClipCursor.argtypes = [ctypes.c_void_p]


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [("dx", wt.LONG), ("dy", wt.LONG), ("mouseData", wt.DWORD), ("dwFlags", wt.DWORD),
                ("time", wt.DWORD), ("dwExtraInfo", ctypes.c_size_t)]


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", wt.WORD), ("wScan", wt.WORD), ("dwFlags", wt.DWORD), ("time", wt.DWORD),
                ("dwExtraInfo", ctypes.c_size_t)]


class _EITHER(ctypes.Union):
    _fields_ = [("mi", MOUSEINPUT), ("ki", KEYBDINPUT)]


class INPUT(ctypes.Structure):
    _fields_ = [("type", wt.DWORD), ("u", _EITHER)]


u32.SendInput.argtypes = [wt.UINT, ctypes.POINTER(INPUT), ctypes.c_int]


def key(vk, up):
    return INPUT(1, _EITHER(ki=KEYBDINPUT(vk, u32.MapVirtualKeyW(vk, 0), 2 if up else 0, 0, 0)))


def mouse(flags, dx=0, dy=0):
    return INPUT(0, _EITHER(mi=MOUSEINPUT(dx, dy, 0, flags, 0, 0)))


def send(event):
    u32.SendInput(1, ctypes.byref(event), ctypes.sizeof(INPUT))


SPACE_DOWN, SPACE_UP = key(0x20, False), key(0x20, True)
LEFT_DOWN, LEFT_UP = mouse(0x0002), mouse(0x0004)

_freq, _count = ctypes.c_int64(), ctypes.c_int64()
k32.QueryPerformanceFrequency(ctypes.byref(_freq))


def qpc():
    """Seconds on the high-resolution counter, the clock the app's timings are given in."""
    k32.QueryPerformanceCounter(ctypes.byref(_count))
    return _count.value / _freq.value


def running_programs():
    out = subprocess.run(["tasklist", "/fo", "csv", "/nh"], capture_output=True, text=True).stdout
    return {line.split('","')[0].strip('"').lower() for line in out.splitlines() if line}


def bring_to_front(hwnd):
    front = u32.GetForegroundWindow()
    if front == hwnd:
        return True
    me = k32.GetCurrentThreadId()
    other = u32.GetWindowThreadProcessId(front, None) if front else 0
    attached = bool(other) and other != me and u32.AttachThreadInput(other, me, True)
    u32.ShowWindow(hwnd, 9)
    u32.BringWindowToTop(hwnd)
    u32.SetForegroundWindow(hwnd)
    if attached:
        u32.AttachThreadInput(other, me, False)
    return u32.GetForegroundWindow() == hwnd


def ms(seconds):
    return seconds * 1000.0


def spread(values):
    """'median 1.2 ms, 90% under 1.8, worst 3.1' for a list of seconds."""
    if not values:
        return "no samples"
    v = sorted(values)
    return f"median {ms(statistics.median(v)):.2f} ms, 90% under {ms(v[int(0.9 * (len(v) - 1))]):.2f}, worst {ms(v[-1]):.2f}"


class Bench:
    def __init__(self, hwnd, pad, app_lines):
        self.hwnd, self.pad, self.app_lines = hwnd, pad, app_lines
        self.gaps = []           # time between two reads of the controller
        self.stopped = None      # why the run stopped early

    def live(self):
        states = [line for line in self.app_lines if line.startswith("live ")]
        return bool(states) and states[-1] == "live yes"

    def ok(self):
        if self.stopped:
            return False
        if u32.GetForegroundWindow() != self.hwnd:
            self.stopped = "the window lost the front"
        elif not self.live():
            self.stopped = "the app stopped taking over"
        return self.stopped is None

    def wait_for(self, condition, timeout):
        """Reads the controller until `condition` holds; the time it was seen, or None."""
        start = last = qpc()
        while True:
            pygame.event.pump()
            now = qpc()
            self.gaps.append(now - last)
            last = now
            if condition():
                return now
            if now - start > timeout:
                return None

    def pause(self, seconds):
        end = qpc() + seconds
        while qpc() < end:
            pygame.event.pump()

    def cross(self):
        return self.pad.get_button(pygame.CONTROLLER_BUTTON_A)

    def r2(self):
        return self.pad.get_axis(pygame.CONTROLLER_AXIS_TRIGGERRIGHT) > 16000

    def right_x(self):
        return self.pad.get_axis(pygame.CONTROLLER_AXIS_RIGHTX) / 32767.0

    def presses(self, down, up, is_on, n):
        """n presses and releases; for each, (pressed at, seen at, released at, seen let go at)."""
        rows = []
        for _ in range(n):
            if not self.ok():
                break
            self.pause(random.uniform(0.03, 0.08))
            t0 = qpc()
            send(down)
            seen = self.wait_for(is_on, 0.3)
            self.pause(random.uniform(0.03, 0.08))
            t1 = qpc()
            send(up)
            gone = self.wait_for(lambda: not is_on(), 0.3)
            rows.append((t0, seen, t1, gone))
            pygame.event.clear()
        return rows

    def movement(self, share, counts_per_second, direction):
        """Moves the mouse sideways at a steady speed for MOVE_FOR seconds, 1000 reports a second
        as a gaming mouse does, and follows the right stick."""
        samples, carry, sent = [], 0.0, 0
        start = qpc()
        end, sent_until = start + MOVE_FOR, start
        while True:
            now = qpc()
            due = 0
            while sent_until + 0.001 <= min(now, end):
                sent_until += 0.001
                carry += counts_per_second * 0.001
                due += int(carry)
                carry -= int(carry)
            if due:
                send(mouse(0x0001, direction * due, 0))
                sent += due
            pygame.event.pump()
            samples.append((qpc(), direction * self.right_x()))
            if now > end + WATCH_AFTER:
                break
        pygame.event.clear()
        steady_part = [x for t, x in samples if start + 0.15 <= t <= end - 0.01]
        steady = statistics.mean(steady_part) if steady_part else 0.0
        rise = fall = None
        if steady >= 0.05:
            rise = next((t - start for t, x in samples if x >= steady / 2), None)
            fall = next((t - end for t, x in samples if t >= end and x <= steady / 2), None)
        return {"share": share, "start": start, "end": end, "sent": sent * direction,
                "rise": rise, "fall": fall, "tilt": steady}

    def wait_centred(self):
        self.wait_for(lambda: abs(self.right_x()) < 0.01, 0.5)


def split(rows, timings, pressed):
    """For presses (pressed=True) or releases: total, Windows, app and driver + reader times."""
    events = [e for e, _ in timings]
    total, windows, app, rest, misses = [], [], [], [], 0
    for t0, seen, t1, gone in rows:
        start, end = (t0, seen) if pressed else (t1, gone)
        if end is None:
            misses += 1
            continue
        total.append(end - start)
        i = bisect.bisect_left(events, start - 0.0005)
        if i < len(events) and events[i] < start + 0.05:
            event, sent = timings[i]
            windows.append(event - start)
            app.append(sent - event)
            rest.append(end - sent)
    return total, windows, app, rest, misses


def report_presses(name, rows, timings):
    for what, pressed in (("press", True), ("release", False)):
        total, windows, app, rest, misses = split(rows, timings, pressed)
        print(f"  {name} {what}: {spread(total)}  ({len(total)} timed{f', {misses} never seen' if misses else ''})")
        if app:
            print(f"      Windows to the app's hook {ms(statistics.median(windows)):.2f} ms, "
                  f"the app {ms(statistics.median(app)):.2f} ms (worst {ms(max(app)):.2f}), "
                  f"driver and reader {ms(statistics.median(rest)):.2f} ms  (medians)")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("exe")
    parser.add_argument("--presses", type=int, default=60, help="presses of Space, and of the left mouse button")
    parser.add_argument("--moves", type=int, default=4, help="mouse movements at each speed")
    parser.add_argument("--sensitivity", type=float, default=5.0, help="mouse speed in the app (yours is 5)")
    args = parser.parse_args()

    busy = REMOTE_PLAY & running_programs()
    if busy:
        print(f"Not running: {', '.join(sorted(busy))} is open and would pass the test presses to the console.")
        return 1

    left, top, _, _ = side_monitor()
    os.environ["SDL_VIDEO_WINDOW_POS"] = f"{left + 60},{top + 60}"
    pygame.display.init()
    pygame.font.init()
    pygame.joystick.init()
    controller.init()
    screen = pygame.display.set_mode((620, 120))
    pygame.display.set_caption("Palmrest latency bench")
    font = pygame.font.Font(None, 30)
    screen.fill((24, 28, 34))
    screen.blit(font.render("Measuring input delay: hands off the keyboard and mouse.", True, (230, 230, 230)), (20, 30))
    screen.blit(font.render("Clicking another window stops it.", True, (160, 170, 180)), (20, 70))
    pygame.display.flip()
    hwnd = pygame.display.get_wm_info()["window"]
    pygame.event.pump()
    before = {pygame.joystick.Joystick(i).get_instance_id() for i in range(pygame.joystick.get_count())}

    program = os.path.basename(sys.executable).lower()
    app = subprocess.Popen([args.exe, "--latency-test", program, "--sensitivity", str(args.sensitivity)],
                           stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    app_lines = []
    reader = threading.Thread(target=lambda: [app_lines.append(line.strip()) for line in app.stdout], daemon=True)
    reader.start()
    try:
        pad, deadline = None, qpc() + 8
        while pad is None and qpc() < deadline and app.poll() is None:
            pygame.event.pump()
            for i in range(pygame.joystick.get_count()):
                j = pygame.joystick.Joystick(i)
                if j.get_instance_id() not in before and controller.is_controller(i) and "4c05" in j.get_guid():
                    pad = controller.Controller(i)
        failed = [line for line in app_lines if line.startswith("FAILED")]
        if pad is None or failed:
            print("FAILED:", failed[0] if failed else "the app's virtual controller never appeared")
            return 1
        bench = Bench(hwnd, pad, app_lines)
        bench.pause(1.0)  # let the new controller settle
        if not bring_to_front(hwnd):
            print("FAILED: could not bring the bench window to the front")
            return 1
        deadline = qpc() + 3
        while not bench.live() and qpc() < deadline:
            bench.pause(0.02)
        if not bench.live():
            print("FAILED: the app did not take over (no 'live yes')")
            return 1

        space = bench.presses(SPACE_DOWN, SPACE_UP, bench.cross, args.presses)
        click = bench.presses(LEFT_DOWN, LEFT_UP, bench.r2, args.presses)
        full = FULL_SPEED_AT_1 / args.sensitivity
        moves = []
        for share in MOVE_SHARES:
            for n in range(args.moves):
                if not bench.ok():
                    break
                bench.wait_centred()
                bench.pause(0.1)
                moves.append(bench.movement(share, full * share, 1 if n % 2 == 0 else -1))
    finally:
        try:
            app.stdin.write("stop\n")
            app.stdin.close()
            app.wait(timeout=5)
        except (OSError, subprocess.TimeoutExpired):
            app.kill()
        reader.join(timeout=2)
        u32.ClipCursor(None)
        pygame.display.quit()

    timings = sorted(tuple(map(float, line.split()[1:3])) for line in app_lines if line.startswith("timing "))
    print(f"Palmrest input delay on this PC: default keys, mouse sensitivity {args.sensitivity:g}, "
          f"read back through SDL {'.'.join(map(str, pygame.get_sdl_version()))}")
    if bench.stopped:
        print(f"STOPPED EARLY: {bench.stopped}. The numbers below are what was measured before that.")
    print(f"  reads of the controller: {spread(bench.gaps)} apart (the resolution of every time below)")
    report_presses("Space -> cross,", space, timings)
    report_presses("left mouse button -> R2,", click, timings)
    # What the app received during each movement. Windows can hold back the last few counts of a
    # movement for a while (to the next movement, at times), so a few missing counts are normal;
    # a movement with many more or fewer than were sent had a real mouse mixed in, and is left out.
    reports = sorted((float(t), int(dx)) for _, t, dx, _ in (line.split() for line in app_lines if line.startswith("mouse ")))
    clean, mixed = [], []
    for m in moves:
        got = [(t, dx) for t, dx in reports if m["start"] <= t <= m["end"] + WATCH_AFTER]
        total = sum(dx for _, dx in got)
        if abs(total - m["sent"]) > max(3, 0.03 * abs(m["sent"])):
            mixed.append(f"{m['share'] * 100:.0f}%: sent {m['sent']}, got {total}")
            continue
        during = [t for t, _ in got if t <= m["end"]]
        m["rate"] = (len(during) - 1) / (during[-1] - during[0]) if len(during) > 2 else 0.0
        m["last"] = got[-1][0] - m["end"] if got else None
        clean.append(m)
    print("  mouse movement -> right stick, from the hand starting / stopping to the stick at half its steady tilt:")
    if mixed:
        print(f"    ({len(mixed)} movements left out, a real mouse probably moved during them: {'; '.join(mixed)})")
    for share in MOVE_SHARES:
        rows = [m for m in clean if m["share"] == share]
        if not rows:
            continue
        rises = [m["rise"] for m in rows if m["rise"] is not None]
        falls = [m["fall"] for m in rows if m["fall"] is not None]
        want = DEADZONE + (1 - DEADZONE) * share ** (1 / CURVE)
        print(f"    {share * 100:>3.0f}% of full speed ({full * share:>5.0f} counts/s): "
              f"start {ms(statistics.median(rises)) if rises else float('nan'):>4.0f} ms, "
              f"stop {ms(statistics.median(falls)) if falls else float('nan'):>4.0f} ms "
              f"(tilt {statistics.mean(m['tilt'] for m in rows):.2f}, the curve asks {want:.2f}; "
              f"reports reaching the app {statistics.median(m['rate'] for m in rows):.0f} a second, "
              f"the last one {ms(statistics.median(m['last'] for m in rows)):+.0f} ms from the stop; {len(rows)} movements)")
    return 1 if bench.stopped else 0


if __name__ == "__main__":
    sys.exit(main())
