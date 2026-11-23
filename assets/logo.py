"""Draws Palmrest's icon: a keyboard key standing in the sea like a small island (a "key"), with
a palm on it, in front of a setting sun.

Run it to write the files beside it again:
  palmrest-1024.png   the picture at full size
  palmrest-256.png    for the window and the README
  palmrest.ico        for the exe, 16 to 256 pixels

Small sizes are drawn with fewer and thicker parts, not shrunk: a palm with seven thin leaves is
a smudge at 16 pixels. Needs pygame.

usage: python assets/logo.py
"""
import io
import math
import os
import struct

os.environ["PYGAME_HIDE_SUPPORT_PROMPT"] = "1"
import pygame

HERE = os.path.dirname(os.path.abspath(__file__))
N = 1536  # everything is drawn this large and shrunk, which smooths the edges


def rgb(code):
    return tuple(int(code[i:i + 2], 16) for i in (1, 3, 5))


def ramp(stops, t):
    """The colour at t along a list of (place, colour) stops."""
    for (t0, c0), (t1, c1) in zip(stops, stops[1:]):
        if t <= t1:
            share = (t - t0) / (t1 - t0) if t1 > t0 else 0
            return tuple(a + (b - a) * share for a, b in zip(rgb(c0), rgb(c1)))
    return rgb(stops[-1][1])


def shaded(stops):
    """A square filled from top to bottom with the colours of the stops."""
    surface = pygame.Surface((N, N), pygame.SRCALPHA)
    for y in range(N):
        pygame.draw.line(surface, ramp(stops, y / (N - 1)) + (255,), (0, y), (N, y))
    return surface


def cut(surface, mask):
    """The surface, kept only where the mask is white."""
    out = surface.copy()
    out.blit(mask, (0, 0), special_flags=pygame.BLEND_RGBA_MULT)
    return out


def at(x, y):
    return (x * N, y * N)


def curve(p0, p1, p2, t):
    return tuple((1 - t) ** 2 * p0[i] + 2 * (1 - t) * t * p1[i] + t * t * p2[i] for i in (0, 1))


def icon(small=False):
    """The icon, N pixels square. `small`: the version for 32 pixels and under."""
    horizon = 0.66
    img = shaded([(0, "#150F36"), (0.34, "#40197A"), (horizon, "#C7307C"), (horizon + 0.001, "#1A1242"), (1, "#0E0A28")])

    # The sun: a disc going down behind the sea, with bands cut out of its lower half.
    cx, cy, r = 0.5, 0.47, 0.30 if small else 0.285
    sun = shaded([(0, "#FFE48F"), (cy - r, "#FFE48F"), (cy + r * 0.2, "#FF8F6B"), (cy + r, "#FF3F8E"), (1, "#FF3F8E")])
    disc = pygame.Surface((N, N), pygame.SRCALPHA)
    pygame.draw.circle(disc, (255, 255, 255, 255), at(cx, cy), r * N)
    if not small:
        for k, y in enumerate([0.50, 0.545, 0.585, 0.62]):
            pygame.draw.rect(disc, (0, 0, 0, 0), pygame.Rect(0, y * N, N, (0.010 + 0.004 * k) * N))
    pygame.draw.rect(disc, (0, 0, 0, 0), pygame.Rect(0, horizon * N, N, N))
    img.blit(cut(sun, disc), (0, 0))

    # The sun's light on the water.
    if not small:
        for y, half, strength in [(0.70, 0.30, 150), (0.735, 0.22, 120), (0.895, 0.17, 110), (0.93, 0.10, 80)]:
            glint = pygame.Surface((N, N), pygame.SRCALPHA)
            pygame.draw.line(glint, rgb("#FF7AA8") + (strength,), at(0.5 - half, y), at(0.5 + half, y), int(0.008 * N))
            img.blit(glint, (0, 0))

    # The key: a keycap seen from the front and a little above.
    width, top, face_h, skirt = (0.60, 0.665, 0.14, 0.085) if small else (0.50, 0.675, 0.125, 0.075)
    body = pygame.Rect((0.5 - width / 2 - 0.022) * N, top * N, (width + 0.044) * N, (face_h + skirt) * N)
    pygame.draw.rect(img, rgb("#6A5FA8"), body, border_radius=int(0.05 * N))
    lower = pygame.Rect(body.x, body.y + body.h * 0.62, body.w, body.h * 0.38)
    pygame.draw.rect(img, rgb("#51478A"), lower, border_bottom_left_radius=int(0.05 * N), border_bottom_right_radius=int(0.05 * N))
    face = pygame.Rect((0.5 - width / 2) * N, (top - 0.012) * N, width * N, face_h * N)
    light = shaded([(0, "#FFFFFF"), (top - 0.012, "#FBF8FF"), (top + face_h, "#D8CEF6"), (1, "#D8CEF6")])
    face_shape = pygame.Surface((N, N), pygame.SRCALPHA)
    pygame.draw.rect(face_shape, (255, 255, 255, 255), face, border_radius=int(0.04 * N))
    img.blit(cut(light, face_shape), (0, 0))

    # The palm, dark against the sun: a leaning trunk and leaves that hang at the ends.
    ink = rgb("#120C2E")
    thick = 1.75 if small else 1.0
    base, bend, crown = (0.455, 0.70), (0.47, 0.50), (0.56, 0.335)
    for i in range(240):
        t = i / 239
        pygame.draw.circle(img, ink, at(*curve(base, bend, crown, t)), (0.021 - 0.010 * t) * thick * N)
    full = [(168, 0.30, 0.17), (138, 0.27, 0.07), (100, 0.22, 0.0), (62, 0.24, 0.03), (28, 0.28, 0.10), (-2, 0.27, 0.19), (205, 0.20, 0.21)]
    few = [(165, 0.31, 0.16), (118, 0.25, 0.02), (62, 0.25, 0.03), (10, 0.29, 0.16), (205, 0.21, 0.20)]
    for angle, length, droop in few if small else full:
        a = math.radians(angle)
        tip = (crown[0] + length * math.cos(a), crown[1] - length * math.sin(a) + droop)
        lift = (crown[0] + 0.55 * length * math.cos(a), crown[1] - 0.55 * length * math.sin(a) - 0.085)
        upper, under = [], []
        for i in range(41):
            t = i / 40
            x, y = curve(crown, lift, tip, t)
            ahead, behind = curve(crown, lift, tip, min(1, t + 0.01)), curve(crown, lift, tip, max(0, t - 0.01))
            dx, dy = ahead[0] - behind[0], ahead[1] - behind[1]
            d = math.hypot(dx, dy) or 1
            w = (0.030 * math.sin(math.pi * min(1, t * 1.02)) ** 0.65 + 0.004 * (1 - t)) * thick
            upper.append(at(x - dy / d * w, y + dx / d * w))
            under.append(at(x + dy / d * w * 0.35, y - dx / d * w * 0.35))
        pygame.draw.polygon(img, ink, upper + under[::-1])
    pygame.draw.circle(img, ink, at(*crown), 0.026 * thick * N)

    # The icon's own outline.
    outline = pygame.Surface((N, N), pygame.SRCALPHA)
    pygame.draw.rect(outline, (255, 255, 255, 255), pygame.Rect(0, 0, N, N), border_radius=int(0.225 * N))
    return cut(img, outline)


def png_bytes(surface):
    data = io.BytesIO()
    pygame.image.save(surface, data, "icon.png")
    return data.getvalue()


def main():
    large, little = icon(), icon(small=True)
    pygame.image.save(pygame.transform.smoothscale(large, (1024, 1024)), os.path.join(HERE, "palmrest-1024.png"))
    pygame.image.save(pygame.transform.smoothscale(large, (256, 256)), os.path.join(HERE, "palmrest-256.png"))
    # An .ico file: a list of the sizes, then each size as a PNG.
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    pictures = [png_bytes(pygame.transform.smoothscale(little if size <= 32 else large, (size, size))) for size in sizes]
    with open(os.path.join(HERE, "palmrest.ico"), "wb") as ico:
        ico.write(struct.pack("<HHH", 0, 1, len(sizes)))
        where = 6 + 16 * len(sizes)
        for size, picture in zip(sizes, pictures):
            ico.write(struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(picture), where))
            where += len(picture)
        for picture in pictures:
            ico.write(picture)
    # A sheet to look the sizes over: not kept with the others.
    return large, little


if __name__ == "__main__":
    main()
