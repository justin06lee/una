#!/usr/bin/env python3
"""Rewrite 8-bit RGB PNGs as RGBA (alpha 255), in place. Stdlib only.

The app icon is an opaque full-bleed square (macOS 26 draws icons with
transparent margins in its glassy legacy style), so rsvg-convert writes it
without an alpha channel, and tauri::generate_context! refuses any bundle
icon PNG that isn't RGBA. Files already RGBA are left alone.

    python3 scripts/png_rgba.py a.png b.png ...
"""

import struct
import sys
import zlib

SIG = b"\x89PNG\r\n\x1a\n"


def chunks(data):
    pos = len(SIG)
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        tag = data[pos + 4 : pos + 8]
        yield tag, data[pos + 8 : pos + 8 + length]
        pos += 12 + length


def chunk(tag, body):
    return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body))


def paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    return b if pb <= pc else c


def unfilter(raw, width, height, bpp):
    stride = width * bpp
    rows, prev, pos = [], bytearray(stride), 0
    for _ in range(height):
        kind, line = raw[pos], bytearray(raw[pos + 1 : pos + 1 + stride])
        pos += 1 + stride
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if kind == 1:
                line[i] = (line[i] + a) & 0xFF
            elif kind == 2:
                line[i] = (line[i] + b) & 0xFF
            elif kind == 3:
                line[i] = (line[i] + (a + b) // 2) & 0xFF
            elif kind == 4:
                line[i] = (line[i] + paeth(a, b, c)) & 0xFF
            elif kind != 0:
                raise ValueError(f"bad filter type {kind}")
        rows.append(line)
        prev = line
    return rows


def to_rgba(path):
    data = open(path, "rb").read()
    if data[:8] != SIG:
        raise ValueError(f"{path}: not a PNG")
    parts = list(chunks(data))
    ihdr = parts[0][1]
    width, height, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", ihdr)
    if color == 6:
        return False  # already RGBA
    if (depth, color, interlace) != (8, 2, 0):
        raise ValueError(f"{path}: only 8-bit non-interlaced RGB is handled")
    raw = zlib.decompress(b"".join(body for tag, body in parts if tag == b"IDAT"))
    out = bytearray()
    for line in unfilter(raw, width, height, 3):
        out.append(0)  # filter: none
        for i in range(0, len(line), 3):
            out += line[i : i + 3] + b"\xff"
    new_ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    # Keep colour-space chunks (sRGB, gAMA, iCCP…); drop the old data.
    keep = [chunk(tag, body) for tag, body in parts[1:] if tag not in (b"IDAT", b"IEND")]
    png = SIG + chunk(b"IHDR", new_ihdr) + b"".join(keep)
    png += chunk(b"IDAT", zlib.compress(bytes(out), 9)) + chunk(b"IEND", b"")
    open(path, "wb").write(png)
    return True


if __name__ == "__main__":
    for p in sys.argv[1:]:
        if to_rgba(p):
            print(f"rgba: {p}")
