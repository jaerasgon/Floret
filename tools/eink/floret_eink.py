#!/usr/bin/env python3
"""
Floret e-ink poller (hybrid pull model) — hardware-agnostic.

Floret's io/eink node POSTs frames to the e-ink relay on the Mac (same-origin
localhost, so macOS/Chrome never gate it). This script runs on the Pi, POLLS
the relay, pulls the latest frame, and paints the panel. It also honours a
`clear` request from the node.

Supports a variety of panels via a small backend abstraction:
  - Waveshare e-Paper (waveshare_epd lib): mono / tri-colour / 4-grey / 7-colour
  - Pimoroni Inky (inky lib): auto-detects the panel (resolution + colour)

Only set the CONFIG block for your hardware. The relay/transport is identical
for every panel; just the final "push pixels" step differs.

Setup: see tools/eink/README.md.
"""

import io
import sys
import time
import requests
from PIL import Image

# ─── CONFIG ──────────────────────────────────────────────────────────────
HOST = "http://192.168.1.221:5173"        # your Mac (where `npm run dev` runs)

BACKEND = "waveshare"        # "waveshare" | "inky"

# Waveshare only:
PANEL = "epd2in13_V4"        # driver module name (waveshare_epd.<PANEL>)
MODE = "mono"                # "mono" | "tricolor" | "gray4" | "color7"
ROTATE = 90                  # 0/90/180/270 — orient the image to the frame
INVERT = False               # flip black/white if the panel reads inverted
# (Inky auto-detects resolution + colour; it only uses ROTATE.)

CHANNEL = "eink"             # which Send-node channel to display
POLL_SECONDS = 1.0           # how often to check the bus for changes
IDLE_SLEEP_S = 30            # sleep the panel after this long with no changes

POLL_URL = f"{HOST}/__floret/bus/{CHANNEL}/poll"
FRAME_URL = f"{HOST}/__floret/bus/{CHANNEL}"


# ─── Image helpers ───────────────────────────────────────────────────────
def fit(rgb: Image.Image, w: int, h: int) -> Image.Image:
    """Rotate + letterbox the render onto a white, panel-sized RGB canvas."""
    img = rgb.rotate(ROTATE, expand=True) if ROTATE else rgb
    img = img.convert("RGB")
    scale = min(w / img.width, h / img.height)
    sw, sh = max(1, int(img.width * scale)), max(1, int(img.height * scale))
    img = img.resize((sw, sh), Image.LANCZOS)
    canvas = Image.new("RGB", (w, h), (255, 255, 255))
    canvas.paste(img, ((w - sw) // 2, (h - sh) // 2))
    return canvas


def to_mono(rgb: Image.Image) -> Image.Image:
    out = rgb.convert("L").convert("1")  # Floyd–Steinberg
    if INVERT:
        from PIL import ImageOps
        out = ImageOps.invert(out.convert("L")).convert("1")
    return out


# ─── Waveshare backend ───────────────────────────────────────────────────
class WaveshareDisplay:
    def __init__(self):
        try:
            mod = __import__("waveshare_epd." + PANEL, fromlist=["EPD"])
        except Exception as e:  # noqa: BLE001
            sys.exit(f"Could not import waveshare_epd.{PANEL}: {e}")
        self.epd = mod.EPD()
        self.width, self.height = self.epd.width, self.epd.height
        self._awake = False

    def label(self) -> str:
        return f"waveshare {PANEL} {self.width}x{self.height} · {MODE}"

    def show(self, rgb: Image.Image) -> None:
        e = self.epd
        img = fit(rgb, self.width, self.height)

        if MODE == "tricolor":
            black, spot = _split_tricolor(img)
            e.init(); e.display(e.getbuffer(black), e.getbuffer(spot)); e.sleep()
            self._awake = False
        elif MODE == "gray4" and hasattr(e, "display_4Gray"):
            e.init(); e.display_4Gray(e.getbuffer_4Gray(img.convert("L"))); e.sleep()
            self._awake = False
        elif MODE == "color7":
            e.init(); e.display(e.getbuffer(_quantize7(img))); e.sleep()
            self._awake = False
        else:  # mono — clean full refresh, keep awake for fast sequential updates
            buf = e.getbuffer(to_mono(img))
            if not self._awake:
                (e.init_fast if hasattr(e, "init_fast") else e.init)()
                self._awake = True
            (e.display_fast if hasattr(e, "display_fast") else e.display)(buf)

    def clear(self) -> None:
        self.epd.init(); self.epd.Clear(); self.epd.sleep()
        self._awake = False

    def sleep(self) -> None:
        if self._awake:
            try: self.epd.sleep()
            except Exception: pass  # noqa: BLE001
            self._awake = False


def _split_tricolor(img: Image.Image):
    """B/W/R(or Y) panels take two 1-bit buffers (black, spot). Dark pixels →
    black; strongly-saturated red/yellow pixels → spot. Needs numpy."""
    import numpy as np
    a = np.asarray(img.convert("RGB"), dtype=np.int16)
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    spot_mask = (r > 130) & (r - g > 60) & (r - b > 60)   # reddish/yellowish
    dark_mask = (a.max(axis=2) < 110) & ~spot_mask
    black = np.where(dark_mask, 0, 255).astype("uint8")
    spot = np.where(spot_mask, 0, 255).astype("uint8")
    return Image.fromarray(black).convert("1"), Image.fromarray(spot).convert("1")


def _quantize7(img: Image.Image) -> Image.Image:
    """7-colour ACeP: dither to the panel's fixed palette."""
    pal = Image.new("P", (1, 1))
    pal.putpalette([
        0, 0, 0, 255, 255, 255, 0, 255, 0, 0, 0, 255,
        255, 0, 0, 255, 255, 0, 255, 128, 0,
    ] + [0, 0, 0] * 249)
    return img.convert("RGB").quantize(palette=pal, dither=Image.FLOYDSTEINBERG)


# ─── Pimoroni Inky backend ───────────────────────────────────────────────
class InkyDisplay:
    def __init__(self):
        try:
            from inky.auto import auto
            self.inky = auto(ask_user=False, verbose=False)
        except Exception as e:  # noqa: BLE001
            sys.exit(f"Could not auto-detect an Inky panel: {e}")
        self.width, self.height = self.inky.resolution

    def label(self) -> str:
        return f"inky {self.width}x{self.height} (auto)"

    def show(self, rgb: Image.Image) -> None:
        img = fit(rgb, self.width, self.height)
        try:
            self.inky.set_image(img, saturation=0.6)  # colour Inky
        except TypeError:
            self.inky.set_image(img)                  # mono/2-3 colour Inky
        self.inky.show()

    def clear(self) -> None:
        self.show(Image.new("RGB", (self.width, self.height), (255, 255, 255)))

    def sleep(self) -> None:
        pass  # Inky holds its image with no sleep API


def make_display():
    return InkyDisplay() if BACKEND == "inky" else WaveshareDisplay()


# ─── Poll loop ───────────────────────────────────────────────────────────
def main() -> None:
    disp = make_display()
    print(f"[floret-eink] poller · {disp.label()} · rot {ROTATE} · {HOST} · #{CHANNEL}", flush=True)
    last_version = None
    last_activity = time.monotonic()
    while True:
        try:
            st = requests.get(POLL_URL, timeout=5).json()
        except Exception:  # noqa: BLE001
            time.sleep(POLL_SECONDS); continue

        # First poll: sync baseline, don't repaint pre-existing state.
        if last_version is None:
            last_version = st["version"]
            time.sleep(POLL_SECONDS); continue

        acted = False
        try:
            if st["has"] and st["version"] != last_version:
                last_version = st["version"]
                png = requests.get(FRAME_URL, timeout=10).content
                disp.show(Image.open(io.BytesIO(png)))
                print(f"[floret-eink] painted v{last_version}", flush=True)
                acted = True
        except Exception as e:  # noqa: BLE001
            print(f"[floret-eink] error: {e}", flush=True)

        now = time.monotonic()
        if acted:
            last_activity = now
        elif now - last_activity > IDLE_SLEEP_S:
            disp.sleep()
        time.sleep(POLL_SECONDS)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n[floret-eink] bye", flush=True)
