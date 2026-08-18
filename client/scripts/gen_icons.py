#!/usr/bin/env python3
"""Generate una's app + tray icons as PNGs, stdlib only (zlib + struct).

App icon: rounded indigo square with a white lowercase "u" glyph.
Tray icon: the "u" glyph alone, black on transparent (macOS template image).

Outputs into apps/desktop/src-tauri/icons/:
  32x32.png  128x128.png  256x256.png  icon.png (512)  tray.png (44, template)
"""

import os
import struct
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
OUT_DIR = os.path.join(HERE, "..", "apps", "desktop", "src-tauri", "icons")

ACCENT = (0x6E, 0x56, 0xCF)  # indigo
WHITE = (0xFF, 0xFF, 0xFF)
BLACK = (0x00, 0x00, 0x00)

SS = 4  # supersampling factor


def write_png(path, size, pixels):
    """pixels: list of rows, each row a bytes of RGBA."""
    raw = b"".join(b"\x00" + row for row in pixels)
    compressed = zlib.compress(raw, 9)

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        c += struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        return c

    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", compressed)
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(png)


def rounded_rect_coverage(x, y, size, radius):
    """1.0 if (x, y) inside the rounded square [0, size]^2, else 0.0."""
    r = radius
    cx = min(max(x, r), size - r)
    cy = min(max(y, r), size - r)
    dx, dy = x - cx, y - cy
    return 1.0 if (dx * dx + dy * dy) <= r * r else 0.0


def u_glyph_coverage(x, y, size):
    """Coverage of a chunky lowercase "u" centered in [0, size]^2.

    Built from two vertical stems and a lower half-annulus (the bowl).
    Geometry is in fractions of the icon size.
    """
    s = size
    stroke = 0.16 * s  # stroke thickness
    top = 0.30 * s  # top of the stems
    center_y = 0.575 * s  # center of the bowl arc
    half_gap = 0.145 * s  # half distance between stem centerlines
    cx = 0.5 * s

    left_cx = cx - half_gap
    right_cx = cx + half_gap
    r_outer = half_gap + stroke / 2.0
    r_inner = half_gap - stroke / 2.0

    # Stems: vertical bars from `top` down to `center_y`.
    for stem_cx in (left_cx, right_cx):
        if abs(x - stem_cx) <= stroke / 2.0 and top <= y <= center_y:
            return 1.0

    # Bowl: lower half annulus centered at (cx, center_y).
    if y >= center_y:
        dx, dy = x - cx, y - center_y
        d2 = dx * dx + dy * dy
        if r_inner * r_inner <= d2 <= r_outer * r_outer:
            return 1.0

    # Right stem descends slightly below the bowl top for the "u" tail.
    if abs(x - right_cx) <= stroke / 2.0 and top <= y <= center_y + 0.02 * s:
        return 1.0

    return 0.0


def render(size, draw_bg=True, glyph_color=WHITE):
    """Render one icon at `size` with SSxSS supersampling."""
    rows = []
    big = size * SS
    radius = 0.225 * big
    for py in range(size):
        row = bytearray()
        for px in range(size):
            # Supersample.
            bg_hits = 0
            glyph_hits = 0
            for sy in range(SS):
                for sx in range(SS):
                    x = px * SS + sx + 0.5
                    y = py * SS + sy + 0.5
                    bg = rounded_rect_coverage(x, y, big, radius) if draw_bg else 0.0
                    if bg:
                        bg_hits += 1
                    if u_glyph_coverage(x, y, big):
                        glyph_hits += 1
            n = SS * SS
            bg_a = bg_hits / n
            gl_a = glyph_hits / n
            if draw_bg:
                # Glyph over accent background, alpha from bg coverage.
                if bg_a <= 0.0:
                    row += bytes((0, 0, 0, 0))
                else:
                    r = ACCENT[0] + (glyph_color[0] - ACCENT[0]) * gl_a
                    g = ACCENT[1] + (glyph_color[1] - ACCENT[1]) * gl_a
                    b = ACCENT[2] + (glyph_color[2] - ACCENT[2]) * gl_a
                    row += bytes((int(r), int(g), int(b), int(255 * bg_a)))
            else:
                # Glyph only (template): color with alpha = coverage.
                row += bytes((*glyph_color, int(255 * gl_a)))
        rows.append(bytes(row))
    return rows


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    for size, name in [(32, "32x32.png"), (128, "128x128.png"), (256, "256x256.png"), (512, "icon.png")]:
        path = os.path.join(OUT_DIR, name)
        write_png(path, size, render(size))
        print(f"wrote {path}")
    tray = os.path.join(OUT_DIR, "tray.png")
    write_png(tray, 44, render(44, draw_bg=False, glyph_color=BLACK))
    print(f"wrote {tray}")


if __name__ == "__main__":
    main()
