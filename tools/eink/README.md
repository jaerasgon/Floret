# Floret → e-ink (generic Send/Receive over the channel bus)

Drive a Raspberry Pi e-paper panel from a Floret patch using the **generic
`io/send` node** — nothing e-ink-specific. Send rasterises the wired layers to
a PNG onto a named channel of the in-app bus; the Pi pulls that channel.

```
Floret io/send (channel "eink") ──POST same-origin localhost──▶ bus (Vite middleware)
                                                                     ▲
Raspberry Pi ── polls /__floret/bus/eink/poll, pulls the PNG ────────┘  ──▶ e-paper panel
```

**Why the bus?** The browser only ever talks to `localhost`, so macOS/Chrome
local-network permissions never apply. The Pi (a plain Python process, no
sandbox) does the cross-network pull. The same bus carries any value (JSON) or
image (PNG) on any channel — `io/receive` reads it back in another Floret tab.

E-ink is static and slow (a clean full refresh is ~1–2s), so this shows one
generative frame at a time and updates every couple of seconds — not animation.

## In Floret (the Mac)

1. `npm run dev` (the bus + `host: true` are already in the Vite config).
2. Add an **`io/send`** node (or load **Samples → Motion → E-ink Clock**). Wire
   your drawing into `data` (alongside the Renderer). Set `channel` to `eink`.
   - `send` (rising edge) → push now · `interval` (s) → auto-push cadence
   - `width`/`height` → the sent resolution (match your panel)
3. Find the Mac's LAN IP: `ipconfig getifaddr en0`.

## On the Raspberry Pi

Enable SPI (`sudo raspi-config` → Interface Options → SPI), then install the
library for your panel:

**Waveshare:**
```sh
sudo apt install -y python3-pip python3-pil python3-numpy git
pip3 install requests
git clone https://github.com/waveshareteam/e-Paper.git ~/e-Paper
cp -r ~/e-Paper/RaspberryPi_JetsonNano/python/lib/waveshare_epd ~/floret/
```

**Pimoroni Inky:**
```sh
sudo apt install -y python3-pip python3-pil
pip3 install requests inky
```

Copy `floret_eink.py` to `~/floret/` and edit its **CONFIG** block:

| setting | meaning |
|---|---|
| `HOST` | `http://<mac-ip>:5173` |
| `CHANNEL` | bus channel to display — match the Send node's `channel` (default `eink`) |
| `BACKEND` | `"waveshare"` or `"inky"` |
| `PANEL` | (Waveshare) module name, e.g. `epd2in13_V4`, `epd7in5_V2`. `ls ~/floret/waveshare_epd` |
| `MODE` | (Waveshare) `mono` · `tricolor` (B/W/R or B/W/Y) · `gray4` · `color7` (ACeP) |
| `ROTATE` | `0/90/180/270` to match how the panel is mounted |
| `INVERT` | flip black/white if the panel reads inverted |

Inky auto-detects resolution + colour — only `HOST`, `BACKEND="inky"`, and
`ROTATE` matter there.

Run it: `python3 ~/floret/floret_eink.py`

### Auto-start (systemd)
`/etc/systemd/system/floret-eink.service`:
```ini
[Unit]
Description=Floret e-ink display
After=network-online.target
Wants=network-online.target
[Service]
WorkingDirectory=/home/pi/floret
ExecStart=/usr/bin/python3 /home/pi/floret/floret_eink.py
Restart=on-failure
RestartSec=10
User=pi
[Install]
WantedBy=multi-user.target
```
```sh
sudo systemctl enable --now floret-eink
journalctl -u floret-eink -f
```

## Notes
- **Refresh:** mono uses a clean full refresh per frame (~1–2s) so moving
  content never ghosts/overlaps. Partial refresh is faster (~0.3s) but ghosts —
  not used here on purpose. Colour/ACeP panels are much slower (10–30s/refresh).
- **Idle:** the panel sleeps after `IDLE_SLEEP_S` of no changes; the next frame
  re-inits. e-ink holds its image with no power.
- **Dithering:** Floret's gradients collapse to the panel's few levels (Floyd–
  Steinberg). Bold, high-contrast patches read best as ink.
- The browser tab feeding the bus must stay open — it's the frame source.
