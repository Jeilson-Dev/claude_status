#!/usr/bin/env python3
"""Generate depleting-gauge tray frames in THREE color sets:
  mono/   black template (macOS tints it) — used for the normal/green state
  yellow/ colored — 'ahead of pace' warning
  red/    colored — 'will block before reset'
Each set has N frames; frame i -> remaining fraction i/(N-1) (12=full, 0=empty).
Pure stdlib. Output: src-tauri/icons/gauge/{mono,yellow,red}/f00.png .."""
import math, os, struct, zlib

W = H = 44
S = 6
N = 13
cx = cy = 22.0
r_out = 18.0
r_in = 12.0

SETS = {
    # name:   (fill_rgb,           fill_a, track_rgb,        track_a)
    "mono":   ((0, 0, 0),          255,    (0, 0, 0),        70),
    "yellow": ((245, 166, 35),     255,    (150, 150, 150),  120),
    "red":    ((224, 50, 46),      255,    (150, 150, 150),  120),
}


def classify(x, y, sweep):
    dx, dy = x - cx, y - cy
    d = math.hypot(dx, dy)
    if not (r_in <= d <= r_out):
        return 0
    ang = math.atan2(dx, -dy) % (2 * math.pi)  # clockwise from top
    return 2 if ang <= sweep else 1


def render_frame(frac, fill_rgb, fill_a, track_rgb, track_a):
    sweep = frac * 2 * math.pi
    data = bytearray(W * H * 4)
    n = S * S
    for py in range(H):
        for px in range(W):
            ar = ag = ab = aa = 0.0
            for sy in range(S):
                yy = py + (sy + 0.5) / S
                for sx in range(S):
                    xx = px + (sx + 0.5) / S
                    c = classify(xx, yy, sweep)
                    if c == 2:
                        r, g, b, a = fill_rgb[0], fill_rgb[1], fill_rgb[2], fill_a
                    elif c == 1:
                        r, g, b, a = track_rgb[0], track_rgb[1], track_rgb[2], track_a
                    else:
                        continue
                    af = a / 255.0
                    ar += r * af
                    ag += g * af
                    ab += b * af
                    aa += af
            out_a = aa / n
            i = (py * W + px) * 4
            if out_a > 0:
                data[i] = min(255, int(round((ar / n) / out_a)))
                data[i + 1] = min(255, int(round((ag / n) / out_a)))
                data[i + 2] = min(255, int(round((ab / n) / out_a)))
                data[i + 3] = min(255, int(round(out_a * 255)))
    return data


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
        raw.append(0)
        raw += data[y * w * 4 : (y + 1) * w * 4]
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(png)


base = os.path.abspath(
    os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons", "gauge")
)
for name, (frgb, fa, trgb, ta) in SETS.items():
    d = os.path.join(base, name)
    os.makedirs(d, exist_ok=True)
    for i in range(N):
        write_png(os.path.join(d, f"f{i:02d}.png"), W, H, render_frame(i / (N - 1), frgb, fa, trgb, ta))
print(f"wrote {len(SETS)} sets x {N} frames under {base}")
