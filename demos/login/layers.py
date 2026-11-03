import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image

folder = Path(sys.argv[1])
lock = np.asarray(Image.open(folder / "greeter.png").convert("RGB")).astype(np.float64) / 255.0
paper = np.asarray(Image.open(folder / "wallpaper.png").convert("RGB")).astype(np.float64) / 255.0

boxes = {
    "clock": (700, 80, 1220, 340),
    "blob": (860, 720, 1060, 910),
    "login": (780, 915, 1140, 1060),
    "corner": (1780, 0, 1920, 40),
}
clean = np.ones(lock.shape[:2], bool)
for x0, y0, x1, y1 in boxes.values():
    clean[y0:y1, x0:x1] = False
clean[540:575, 950:980] = False

veil = np.zeros_like(lock)
for c in range(3):
    p = paper[..., c][clean]
    l = lock[..., c][clean]
    design = np.stack([p, np.ones_like(p)], axis=1)
    (k, b), *_ = np.linalg.lstsq(design, l, rcond=None)
    veil[..., c] = paper[..., c] * k + b
    print("channel", c, "keep", round(k, 3), "add", round(b, 3))

Image.fromarray((veil.clip(0, 1) * 255).round().astype(np.uint8), "RGB").save(folder / "veiled.png")

placed = {}
for name, (x0, y0, x1, y1) in boxes.items():
    l = lock[y0:y1, x0:x1]
    v = veil[y0:y1, x0:x1]
    alpha = (np.abs(l - v).max(axis=2) - 0.012).clip(0, None) / 0.12
    alpha = alpha.clip(0, 1)
    safe = np.maximum(alpha, 1e-3)[..., None]
    color = ((l - v * (1 - alpha[..., None])) / safe).clip(0, 1)
    rgba = np.dstack([color, alpha]) * 255.0
    Image.fromarray(rgba.round().astype(np.uint8), "RGBA").save(folder / f"{name}.png")
    placed[name] = {"x": x0, "y": y0, "w": x1 - x0, "h": y1 - y0}
(folder / "layers.json").write_text(json.dumps(placed))
print(json.dumps(placed))
