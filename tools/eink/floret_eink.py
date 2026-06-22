#!/usr/bin/env python3
"""
Floret → Waveshare e-paper, ambient pull.

Polls the Floret dev server's frame relay (the eink-relay Vite middleware)
and pushes the latest render to a Waveshare e-paper panel on a slow timer.
E-ink is static + slow, so this shows one generative frame at a time and
refreshes every few minutes.

Setup: see tools/eink/README.md. Set HOST below to your Floret machine's
LAN IP, and PANEL to your Waveshare model.
"""

import io
import time
import sys
import requests
from PIL import Image

# ─── Config ──────────────────────────────────────────────────────────────
# Floret machine's LAN address (where `npm run dev` runs, bound host:true).
HOST = "http://192.168.1.50:5173"          # ← set to your Mac's IP
FRAME_URL = HOST + "/__floret/frame.png"

# Waveshare driver module name from the `waveshare_epd` package, e.g.
# 'epd7in5_V2' (7.5" mono), 'epd5in65f' (5.65" 7-color), 'epd2in13_V4'.
# Find yours: ls ~/e-Paper/RaspberryPi_JetsonNano/python/lib/waveshare_epd
PANEL = "epd7in5_V2"

# 'mono'   → 1-bit black/white, Floyd–Steinberg dither (most panels)
# 'gray4'  → 4-level grayscale (panels with epd.init_4Gray/display_4Gray)
# 'color7' → 7-color ACeP panels (epdXin Xf); PIL quantises to the palette
COLOR_MODE = "mono"

REFRESH_SECONDS = 600       # how often to repaint (10 min). E-ink wear: keep slow.
POLL_SECONDS = 10           # retry cadence while waiting for the first frame
INVERT = False              # flip black/white if your panel reads inverted
ROTATE = 0                  # 0/90/180/270 — orient the image to the frame

# ─── Driver load ─────────────────────────────────────────────────────────
try:
    epd_module = __import__("waveshare_epd." + PANEL, fromlist=["EPD"])
except Exception as e:  # noqa: BLE001
    sys.exit(
        f"Could not import waveshare_epd.{PANEL}: {e}\n"
        "Install the Waveshare lib and check the module name "
        "(see README.md)."
    )

epd = epd_module.EPD()
PW, PH = epd.width, epd.height


def fit_to_panel(img: Image.Image) -> Image.Image:
    """Letterbox the render onto a white panel-sized canvas, preserving aspect."""
    if ROTATE:
        img = img.rotate(ROTATE, expand=True)
    img = img.convert("RGB")
    scale = min(PW / img.width, PH / img.height)
    w, h = max(1, int(img.width * scale)), max(1, int(img.height * scale))
    img = img.resize((w, h), Image.LANCZOS)
    canvas = Image.new("RGB", (PW, PH), (255, 255, 255))
    canvas.paste(img, ((PW - w) // 2, (PH - h) // 2))
    return canvas


def quantize(img: Image.Image):
    """Map the RGB render to what the panel can show (dithered)."""
    if COLOR_MODE == "mono":
        out = img.convert("L").convert("1")        # PIL Floyd–Steinberg
        if INVERT:
            from PIL import ImageOps
            out = ImageOps.invert(out.convert("L")).convert("1")
        return out
    if COLOR_MODE == "gray4":
        # Quantise to 4 grays; the 4Gray display path expects an 'L' image.
        return img.convert("L")
    if COLOR_MODE == "color7":
        # ACeP: let PIL dither to the panel's 7-colour palette.
        pal = Image.new("P", (1, 1))
        pal.putpalette([
            0, 0, 0, 255, 255, 255, 0, 255, 0, 0, 0, 255,
            255, 0, 0, 255, 255, 0, 255, 128, 0,
        ] + [0, 0, 0] * 249)
        return img.convert("RGB").quantize(palette=pal, dither=Image.FLOYDSTEINBERG)
    raise SystemExit(f"unknown COLOR_MODE {COLOR_MODE!r}")


def show(img: Image.Image) -> None:
    """Full refresh, then sleep the panel (e-ink wear + power)."""
    epd.init()
    framed = quantize(fit_to_panel(img))
    if COLOR_MODE == "gray4" and hasattr(epd, "display_4Gray"):
        epd.display_4Gray(epd.getbuffer_4Gray(framed))
    else:
        epd.display(epd.getbuffer(framed))
    epd.sleep()


def fetch_frame() -> Image.Image | None:
    try:
        r = requests.get(FRAME_URL, timeout=10)
        if r.status_code == 404:
            return None  # Floret up but no frame published yet
        r.raise_for_status()
        return Image.open(io.BytesIO(r.content))
    except requests.RequestException as e:  # noqa: BLE001
        print(f"[floret-eink] fetch failed: {e}", flush=True)
        return None


def main() -> None:
    print(f"[floret-eink] {PANEL} {PW}x{PH} · {COLOR_MODE} · {FRAME_URL}", flush=True)
    last_bytes = None
    while True:
        img = fetch_frame()
        if img is None:
            time.sleep(POLL_SECONDS)
            continue
        # Skip a repaint if the frame is byte-identical to the last one.
        buf = io.BytesIO()
        img.save(buf, format="PNG")
        if buf.getvalue() == last_bytes:
            time.sleep(REFRESH_SECONDS)
            continue
        last_bytes = buf.getvalue()
        try:
            show(img)
            print(f"[floret-eink] painted @ {time.strftime('%H:%M:%S')}", flush=True)
        except Exception as e:  # noqa: BLE001
            print(f"[floret-eink] display error: {e}", flush=True)
        time.sleep(REFRESH_SECONDS)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        try:
            epd.init(); epd.Clear(); epd.sleep()
        except Exception:  # noqa: BLE001
            pass
        print("\n[floret-eink] bye", flush=True)
