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

## Geometry crates (native engine)

Rust crates compiled into the native app for the `geometry/*` nodes
(`native/crates/floret-engine/src/nodes/geometry/`). Used unmodified, from
crates.io.

- **delaunator** — Delaunay triangulation (`geometry/cells`).
  - **Copyright:** © 2018 Mapbox (author Vladimir Agafonkin)
  - **License:** ISC
  - **Project:** https://github.com/mourner/delaunator-rs
- **robust** — adaptive-precision geometric predicates (used by delaunator).
  - **Copyright:** © the GeoRust contributors
  - **License:** MIT OR Apache-2.0
  - **Project:** https://github.com/georust/robust
- **i_overlay** — polygon boolean operations and offsetting
  (`geometry/boolean`, `geometry/path-ops`).
  - **Copyright:** © Nail Sharipov (iShape-Rust)
  - **License:** MIT OR Apache-2.0
  - **Project:** https://github.com/iShape-Rust/iOverlay
- **i_float**, **i_shape**, **i_tree**, **i_key_sort** — support crates
  of i_overlay.
  - **Copyright:** © Nail Sharipov (iShape-Rust)
  - **License:** MIT
  - **Project:** https://github.com/iShape-Rust
- **libm** — floating-point maths (used by i_float).
  - **Copyright:** © the Rust Project contributors
  - **License:** MIT
  - **Project:** https://github.com/rust-lang/compiler-builtins

ISC, MIT and Apache-2.0 are permissive and compatible with redistribution.
The obligation met here: the copyright and license notices are kept (in
each crate's source and in this file).

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
  (File → Export Video / GIF) in the Windows and Linux builds. **The macOS
  build doesn't include it**: there, video plays and exports through
  Apple's AVFoundation / VideoToolbox, which macOS licenses for H.264.
- **Copyright:** © Cisco Systems, Inc. (OpenH264); © Ralf Biedert and
  contributors (`openh264` crates)
- **License:** BSD-2-Clause.
- **Project:** https://github.com/cisco/openh264 ·
  https://github.com/ralfbiedert/openh264-rs

BSD-2-Clause asks for the copyright notice and disclaimer in binary
distributions. Note: Cisco's royalty-free H.264 patent coverage applies to
the prebuilt binaries Cisco distributes, not to OpenH264 compiled from
source, as here. Before releasing the Windows / Linux builds, switch them
to the OS's own codecs (Windows Media Foundation) or to Cisco's binary
downloaded at runtime, or check H.264 patent licensing.

---

## Inter (interface font)

- **What:** the Inter typeface, Regular and Medium weights, used for the
  native app's interface text (`native/crates/floret-app/src/theme.rs`).
- **Files vendored:** `native/crates/floret-app/assets/fonts/Inter-Regular.ttf`
  and `Inter-Medium.ttf` — Inter 4.001, the static TTFs as published in the
  `@expo-google-fonts/inter` npm package (0.4.2; the Google Fonts build of
  Inter), unmodified.
- **Copyright:** © 2016 The Inter Project Authors
- **License:** SIL Open Font License 1.1 — full text in
  [`native/crates/floret-app/assets/fonts/LICENSE-Inter.txt`](native/crates/floret-app/assets/fonts/LICENSE-Inter.txt).
- **Project:** https://github.com/rsms/inter

The OFL allows bundling the fonts with software; the obligation met here is
keeping the copyright notice and license with them. The fonts aren't sold on
their own.

---

## Phosphor Icons via the `egui-phosphor` crate

- **What:** the Phosphor icon font (Regular weight), compiled into the native
  app through the `egui-phosphor` crate for the toolbar, preview header and
  status bar icons. No files vendored.
- **Copyright:** © Phosphor Icons (the icons and font); © 2023 Romet
  Tagobert (`egui-phosphor`)
- **License:** Phosphor Icons: MIT. `egui-phosphor`: MIT OR Apache-2.0.
- **Project:** https://github.com/phosphor-icons/core ·
  https://github.com/amPerl/egui-phosphor

The MIT terms are met by keeping the copyright and permission notice with
binary distributions (an app bundle's licenses file / About box).
