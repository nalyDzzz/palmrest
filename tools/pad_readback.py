"""Check that the virtual controller the app plugs in carries what the app sent.

Starts `<exe> --pad-test`, which steps through every control and prints each step's name, and
reads the controller through SDL (pygame) the way a game or Remote Play program would.

usage: python pad_readback.py <path to ps5-remote-sim.exe>
"""
import os
import subprocess
import sys
import threading
import time

os.environ["PYGAME_HIDE_SUPPORT_PROMPT"] = "1"
os.environ["SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS"] = "1"
import pygame
from pygame._sdl2 import controller

BUTTONS = {
    "cross": pygame.CONTROLLER_BUTTON_A, "circle": pygame.CONTROLLER_BUTTON_B,
    "square": pygame.CONTROLLER_BUTTON_X, "triangle": pygame.CONTROLLER_BUTTON_Y,
    "l1": pygame.CONTROLLER_BUTTON_LEFTSHOULDER, "r1": pygame.CONTROLLER_BUTTON_RIGHTSHOULDER,
    "l3": pygame.CONTROLLER_BUTTON_LEFTSTICK, "r3": pygame.CONTROLLER_BUTTON_RIGHTSTICK,
    "options": pygame.CONTROLLER_BUTTON_START, "create": pygame.CONTROLLER_BUTTON_BACK,
    "ps": pygame.CONTROLLER_BUTTON_GUIDE, "touchpad": 20,
    "dpad_up": pygame.CONTROLLER_BUTTON_DPAD_UP, "dpad_down": pygame.CONTROLLER_BUTTON_DPAD_DOWN,
    "dpad_left": pygame.CONTROLLER_BUTTON_DPAD_LEFT, "dpad_right": pygame.CONTROLLER_BUTTON_DPAD_RIGHT,
}
AXES = {"lx": pygame.CONTROLLER_AXIS_LEFTX, "ly": pygame.CONTROLLER_AXIS_LEFTY,
        "rx": pygame.CONTROLLER_AXIS_RIGHTX, "ry": pygame.CONTROLLER_AXIS_RIGHTY,
        "l2": pygame.CONTROLLER_AXIS_TRIGGERLEFT, "r2": pygame.CONTROLLER_AXIS_TRIGGERRIGHT}
# step name -> axis values expected (SDL: stick down and right are positive)
STICKS = {"l2": {"l2": 1.0}, "r2": {"r2": 1.0}, "left_stick_right": {"lx": 1.0}, "left_stick_up": {"ly": -1.0},
          "right_stick_left": {"rx": -1.0}, "right_stick_down": {"ry": 1.0}, "right_stick_half_right": {"rx": 0.5}}


def read(pad):
    pygame.event.pump()
    pressed = {name for name, b in BUTTONS.items() if pad.get_button(b)}
    axes = {name: pad.get_axis(a) / 32767.0 for name, a in AXES.items()}
    return pressed, axes


def main():
    exe = sys.argv[1]
    pygame.display.init(); pygame.joystick.init(); controller.init()
    before = pygame.joystick.get_count()
    proc = subprocess.Popen([exe, "--pad-test"], stdout=subprocess.PIPE, text=True)
    steps = []
    threading.Thread(target=lambda: [steps.append((time.perf_counter(), l.strip())) for l in proc.stdout], daemon=True).start()

    pad, deadline = None, time.perf_counter() + 6
    while pad is None and time.perf_counter() < deadline:
        pygame.event.pump()
        for i in range(pygame.joystick.get_count()):
            if controller.is_controller(i) and "4c05" in pygame.joystick.Joystick(i).get_guid():
                pad = controller.Controller(i)
                print("found:", controller.name_forindex(i), "| controllers before:", before)
        time.sleep(0.05)
    if pad is None:
        print("FAILED: the virtual controller never appeared"); proc.kill(); return 1

    done, bad, checked = 0, 0, 0
    while proc.poll() is None or done < len(steps):
        if done < len(steps):
            at, name = steps[done]
            if time.perf_counter() - at < 0.15:      # let the state arrive
                time.sleep(0.01); continue
            done += 1
            if name in ("done",) or name.startswith("FAILED"):
                if name.startswith("FAILED"): print(name); bad += 1
                continue
            pressed, axes = read(pad)
            want_buttons = {name} if name in BUTTONS else set()
            want_axes = {k: 0.0 for k in AXES}; want_axes.update(STICKS.get(name, {}))
            ok = pressed == want_buttons and all(abs(axes[k] - v) < 0.03 for k, v in want_axes.items())
            checked += 1; bad += not ok
            if not ok:
                print(f"MISMATCH {name}: buttons {sorted(pressed)} axes { {k: round(v, 2) for k, v in axes.items() if abs(v) > 0.02} }")
        else:
            time.sleep(0.01)
    time.sleep(0.6); pygame.event.pump()
    left = sum(1 for i in range(pygame.joystick.get_count()) if "4c05" in pygame.joystick.Joystick(i).get_guid())
    print(f"steps checked: {checked} | mismatches: {bad} | virtual controllers left plugged in after exit: {left}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
