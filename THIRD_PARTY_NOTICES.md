# Third-Party Notices

Floret bundles and/or depends on third-party software. Each component is the
property of its respective owners and is used under the license noted below.
This file is informational; the authoritative terms are in each component's
own license text.

---

## MediaPipe (Google AI Edge)

- **What:** `@mediapipe/tasks-vision` runtime (npm dependency) and the
  self-hosted WebAssembly + pre-trained landmarker models vendored under
  [`public/mediapipe/`](public/mediapipe/) — used by the `input/landmarks`
  node for hand / face / pose tracking.
- **Files vendored:**
  - `public/mediapipe/wasm/` — Tasks-Vision WASM runtime (copied from the
    `@mediapipe/tasks-vision` package, unmodified)
  - `public/mediapipe/models/hand_landmarker.task`
  - `public/mediapipe/models/face_landmarker.task`
  - `public/mediapipe/models/pose_landmarker_lite.task`
  - Models downloaded unmodified from
    `https://storage.googleapis.com/mediapipe-models/`
- **Copyright:** © The MediaPipe Authors
- **License:** Apache License 2.0 — full text and attribution in
  [`public/mediapipe/LICENSE`](public/mediapipe/LICENSE) and
  [`public/mediapipe/NOTICE`](public/mediapipe/NOTICE).
- **Project:** https://github.com/google-ai-edge/mediapipe

Apache-2.0 is permissive and compatible with redistribution. The obligations
met here: the license and NOTICE are retained alongside the redistributed
assets, the assets are unmodified, and no trademark/endorsement is implied.

---

## gif (Rust crate)

- **What:** the `gif` crate (with its `color_quant` NeuQuant quantiser and
  `weezl` LZW coder), used by `floret-video` to write animated GIFs for
  File → Export Video / GIF. Compiled into the app; no files vendored.
- **Copyright:** © 2015 nwin and the image-rs developers (`gif`); © 2016
  PistonDevelopers (`color_quant`); © 2020 HeroicKatora (`weezl`)
- **License:** `gif` and `weezl`: MIT OR Apache-2.0; `color_quant`: MIT.
- **Project:** https://github.com/image-rs/image-gif

The MIT terms are met by keeping the copyright and permission notice with
binary distributions (an app bundle's licenses file / About box).

---

## OpenH264 (Cisco) via the `openh264` crate

- **What:** Cisco's OpenH264 codec, compiled from the source the
  `openh264-sys2` crate bundles, through the `openh264` Rust bindings. Used
  by `floret-video` to decode H.264 video files and to encode MP4 exports
  (File → Export Video / GIF).
- **Copyright:** © Cisco Systems, Inc. (OpenH264); © Ralf Biedert and
  contributors (`openh264` crates)
- **License:** BSD-2-Clause.
- **Project:** https://github.com/cisco/openh264 ·
  https://github.com/ralfbiedert/openh264-rs

BSD-2-Clause asks for the copyright notice and disclaimer in binary
distributions. Note: Cisco's royalty-free H.264 patent coverage applies to
the prebuilt binaries Cisco distributes, not to OpenH264 compiled from
source, as here; check H.264 patent licensing before a commercial release.
