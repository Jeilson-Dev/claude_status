#!/usr/bin/env python3
"""Generate a monochrome 'gauge' tray icon (template image: black + alpha).
Pure stdlib (zlib for PNG). Supersampled for anti-aliasing.
Output: src-tauri/icons/tray-icon.png (44x44, @2x of a 22pt menu-bar slot)."""
import math, os, struct, zlib

W = H = 44
S = 6  # supersample factor
HW, HH = W * S, H * S

cx = cy = 22.0
r_out = 18.0
r_in = 11.5
# speedometer opening at the bottom (screen-down). Angles in math orientation.
gap_lo = math.radians(238)
gap_hi = math.radians(302)
# a needle pointing up-right (~58 deg), from center outwards
needle_ang = math.radians(58)
needle_len = 16.0
needle_half = 1.6


def in_ring(x, y):
    dx, dy = x - cx, y - cy
    d = math.hypot(dx, dy)
    if not (r_in <= d <= r_out):
        return False
    ang = math.atan2(-dy, dx) % (2 * math.pi)
    return not (gap_lo <= ang <= gap_hi)


def in_needle(x, y):
    # distance from point to the segment center->tip
    tx = cx + math.cos(needle_ang) * needle_len
    ty = cy - math.sin(needle_ang) * needle_len
    vx, vy = tx - cx, ty - cy
    wx, wy = x - cx, y - cy
    seg2 = vx * vx + vy * vy
    t = max(0.0, min(1.0, (wx * vx + wy * vy) / seg2))
    px, py = cx + t * vx, cy + t * vy
    return math.hypot(x - px, y - py) <= needle_half


acc = [0] * (W * H)
for Y in range(HH):
    yy = (Y + 0.5) / S
    for X in range(HW):
        xx = (X + 0.5) / S
        if in_ring(xx, yy) or in_needle(xx, yy):
            acc[(Y // S) * W + (X // S)] += 1

maxc = S * S
rgba = bytearray(W * H * 4)
for i in range(W * H):
    a = round(acc[i] * 255 / maxc)
    rgba[i * 4 + 3] = a  # black (0,0,0) with computed alpha


def write_png(path, w, h, data):
    def chunk(typ, payload):
        return (
            struct.pack(">I", len(payload))
            + typ
            + payload
            + struct.pack(">I", zlib.crc32(typ + payload) & 0xFFFFFFFF)
        )

    raw = bytearray()
    for y in range(h):
        raw.append(0)  # filter: none
        raw += data[y * w * 4 : (y + 1) * w * 4]
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(png)


out = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons", "tray-icon.png")
out = os.path.abspath(out)
write_png(out, W, H, rgba)
print("wrote", out, os.path.getsize(out), "bytes")
