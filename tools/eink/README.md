# Floret → Waveshare e-ink (ambient pull)

Show Floret's render on a Raspberry Pi e-paper panel. E-ink is static and
slow (full refresh ~1–4s, color panels longer), so this displays one
generative frame and refreshes every few minutes — not animation.

```
Floret (browser)  ──POST /__floret/frame──▶  Vite dev relay (in-memory)
                                                     ▲
Raspberry Pi  ──GET /__floret/frame.png (timer)──────┘  ──▶ Waveshare panel
```

## 1. On the Floret machine (Mac)

1. Run the dev server — it already binds to the LAN (`host: true`) and mounts
   the relay (`eink-relay.vite.mjs`):
   ```sh
   npm run dev
   ```
2. Find this machine's LAN IP (`ipconfig getifaddr en0` on macOS).
3. Open Floret in a browser and **leave the tab open** (it's the frame
   source). In the devtools console, start publishing:
   ```js
   __floretEink.start()                 // every 60s
   __floretEink.start({ intervalMs: 300000 })   // every 5 min
   __floretEink.stop()
   ```
   Whatever the preview is rendering gets POSTed to the relay at that cadence.

Sanity check from the Pi (or anywhere on the LAN):
```sh
curl -s http://<mac-ip>:5173/__floret/frame-age   # {"has":true,"ageMs":...}
curl -so frame.png http://<mac-ip>:5173/__floret/frame.png && open frame.png
```

## 2. On the Raspberry Pi

Enable SPI (`sudo raspi-config` → Interface Options → SPI), then:

```sh
sudo apt install -y python3-pip python3-pil
pip3 install requests
# Waveshare driver library:
git clone https://github.com/waveshareteam/e-Paper.git ~/e-Paper
pip3 install ~/e-Paper/RaspberryPi_JetsonNano/python/   # provides `waveshare_epd`
```

Edit the top of `floret_eink.py`:
- `HOST` → `http://<mac-ip>:5173`
- `PANEL` → your module name. List them:
  `ls ~/e-Paper/RaspberryPi_JetsonNano/python/lib/waveshare_epd`
  (e.g. `epd7in5_V2`, `epd2in13_V4`, `epd5in65f`).
- `COLOR_MODE` → `mono` (B/W), `gray4` (4-level), or `color7` (ACeP).
- `REFRESH_SECONDS`, `ROTATE`, `INVERT` as needed.

Run it:
```sh
python3 floret_eink.py
```

### Auto-start (systemd)
`/etc/systemd/system/floret-eink.service`:
```ini
[Unit]
Description=Floret e-ink display
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/bin/python3 /home/pi/floret/floret_eink.py
Restart=on-failure
User=pi

[Install]
WantedBy=multi-user.target
```
```sh
sudo systemctl enable --now floret-eink
journalctl -u floret-eink -f
```

## Notes / gotchas
- **B/W/Red panels** (e.g. `epd7in5b_V2`) take *two* buffers:
  `epd.display(epd.getbuffer(black), epd.getbuffer(red))`. The script's
  `show()` handles mono/gray4/color7 — add a red-channel split if you have a
  tri-color panel.
- **Dithering is everything.** Floret's gradients must collapse to the
  panel's few levels; the script uses PIL Floyd–Steinberg. For a cleaner
  look, design high-contrast patches (the render reads as ink, not photo).
- **E-ink wear:** the script does full refreshes and `epd.sleep()`s between
  them, and skips repaints when the frame is unchanged. Don't drop
  `REFRESH_SECONDS` too low.
- **The browser tab must stay open** — it's the frame source. (A headless
  capture on the Mac, or a GPU-resident render, would remove that; future
  work.)
- This rides on `vite dev`. For a permanent install you'd serve the relay
  from a small standalone process instead.
