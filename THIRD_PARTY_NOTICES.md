# Third-Party Notices

Floret bundles and depends on third-party software, fonts and models. Each
component is the property of its respective owners and is used under the
licence noted below. This file is informational; the authoritative terms are
in each component's own licence text.

Floret itself is free and open source under the Apache License 2.0: see
[`LICENSE`](LICENSE) and [`NOTICE`](NOTICE). The same texts are in the
app under **Help → Licenses…**, and in the macOS app bundle under
`Floret.app/Contents/Resources/`.

These notices cover the native app (`native/`). The old TypeScript web app in
`web-legacy/` is a reference only and is not
part of what ships; its npm dependencies are not listed.

Contents:

1. MediaPipe tracking models
2. Fonts: Inter, the engine's text fonts, egui's built-in fonts
3. Icons: Phosphor
4. Rust crates called out by name: geometry, GIF, video, audio
5. Every other Rust crate: [`native/licenses/crates.md`](native/licenses/crates.md)

---

## 1. MediaPipe (Google AI Edge)

- **What:** the pre-trained hand, face and pose landmarker models behind the
  `input/landmarks` node. They are embedded in the native app as `.task`
  files (`include_bytes!` in `native/crates/floret-input/src/tracking/`) and
  run by Floret's own small TensorFlow Lite interpreter
  (`native/crates/floret-input/src/nn/`). The MediaPipe runtime itself
  (the npm / WebAssembly package) is not part of the native app.
- **Files embedded:**
  - `public/mediapipe/models/hand_landmarker.task`
  - `public/mediapipe/models/face_landmarker.task`
  - `public/mediapipe/models/pose_landmarker_lite.task`
  - downloaded unmodified from `https://storage.googleapis.com/mediapipe-models/`
- **Copyright:** © The MediaPipe Authors
- **Licence:** Apache License 2.0. Full text and attribution in
  [`public/mediapipe/LICENSE`](public/mediapipe/LICENSE) and
  [`public/mediapipe/NOTICE`](public/mediapipe/NOTICE).
- **Project:** https://github.com/google-ai-edge/mediapipe

Apache-2.0 is permissive and compatible with redistribution. The obligations
met here: the licence and NOTICE are kept with the redistributed models, the
models are unmodified, and no trademark or endorsement is implied.

---

## 2. Fonts

### Inter (interface font)

- **What:** the Inter typeface, Regular and Medium weights, used for the
  interface text (`native/crates/floret-app/src/theme.rs`).
- **Files:** `native/crates/floret-app/assets/fonts/Inter-Regular.ttf` and
  `Inter-Medium.ttf`: Inter 4.001, the static TTFs as published in the
  `@expo-google-fonts/inter` npm package (0.4.2; the Google Fonts build of
  Inter), unmodified.
- **Copyright:** © 2020 The Inter Project Authors
- **Licence:** SIL Open Font License 1.1. Full text in
  [`native/crates/floret-app/assets/fonts/LICENSE-Inter.txt`](native/crates/floret-app/assets/fonts/LICENSE-Inter.txt).
- **Project:** https://github.com/rsms/inter

### Text-node fonts built into the engine

The engine embeds these so the `text/*` nodes draw the same on every machine.
They are in `native/crates/floret-engine/fonts/`, unmodified, each with its
licence file there.

- **DejaVu Sans Mono** (Regular, Bold, Oblique, Bold Oblique)
  - **Copyright:** © 2003 Bitstream, Inc. (Bitstream Vera); the DejaVu
    changes are in the public domain.
  - **Licence:** Bitstream Vera Font License. Text in `LICENSE-DejaVu.txt`.
  - **Project:** https://dejavu-fonts.github.io/
- **Liberation Sans** and **Liberation Serif** (Regular, Bold, Italic, Bold
  Italic)
  - **Copyright:** © 2012 Red Hat, Inc. (Reserved Font Name Liberation);
    digitised data © 2010 Google Corporation (Reserved Font Names Arimo,
    Tinos, Cousine)
  - **Licence:** SIL Open Font License 1.1. Text in `LICENSE-Liberation.txt`.
  - **Project:** https://github.com/liberationfonts
- **Roboto Flex** (variable)
  - **Copyright:** © 2017 The Roboto Flex Project Authors
  - **Licence:** SIL Open Font License 1.1. Text in `LICENSE-RobotoFlex.txt`.
  - **Project:** https://github.com/googlefonts/roboto-flex

### Fonts built into egui

The egui interface toolkit (the `epaint_default_fonts` crate, licence
`(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`) embeds four fonts that
Floret uses as fallbacks for symbols and emoji and for the monospace text of
the code panels. The licence texts are in
`native/crates/floret-app/assets/licenses/`.

- **Hack** (monospace): © 2018 Source Foundry Authors, MIT licence; derived
  from Bitstream Vera and DejaVu. `Hack.txt`.
  https://github.com/source-foundry/Hack
- **Noto Emoji** (emoji): © The Noto Project Authors, SIL Open Font License
  1.1. `Noto-Emoji-OFL.txt`. https://github.com/googlefonts/noto-emoji
- **emoji-icon-font** (symbols): © 2014 John Slegers, MIT licence.
  `emoji-icon-font-MIT.txt`. https://github.com/jslegers/emoji-icon-font
- **Ubuntu Light** (fallback text): © Canonical Ltd, Ubuntu Font Licence 1.0.
  `Ubuntu-Font-Licence.txt`. https://design.ubuntu.com/font

The OFL allows bundling the fonts with software; the obligation met here is
keeping the copyright notices and licences with them. The fonts are not sold
on their own. The Ubuntu Font Licence likewise allows bundling and
redistribution with the licence text kept.

---

## 3. Phosphor Icons via the `egui-phosphor` crate

- **What:** the Phosphor icon font (Regular weight), compiled into the app
  through the `egui-phosphor` crate for the toolbar, preview header and
  status bar icons. No font files are vendored.
- **Copyright:** © 2020 Phosphor Icons (the icons and font); © 2023 Romet
  Tagobert (`egui-phosphor`)
- **Licence:** Phosphor Icons: MIT. `egui-phosphor`: MIT OR Apache-2.0, used
  under MIT. Texts in `native/crates/floret-app/assets/licenses/`
  (`Phosphor-MIT.txt`, `egui-phosphor-MIT.txt`).
- **Project:** https://github.com/phosphor-icons/core ·
  https://github.com/amPerl/egui-phosphor

The MIT terms are met by keeping the copyright and permission notice with
binary distributions (the app bundle's licence files and Help → Licenses…).

---

## 4. Rust crates called out by name

All Rust dependencies are compiled into the app unmodified, from crates.io,
and are listed in full in [`native/licenses/crates.md`](native/licenses/crates.md)
(name, version, licence, repository). The licence texts and the copyright
notices the crates ship are in
[`native/licenses/licence-texts.txt`](native/licenses/licence-texts.txt).
Both files are generated from `Cargo.lock` by `tools/gen-licenses.py` and
checked by a test, so they cannot fall behind. The ones below get a note of
their own.

### Geometry (`geometry/*` nodes)

Used in `native/crates/floret-engine/src/nodes/geometry/`.

- **delaunator**: Delaunay triangulation (`geometry/cells`).
  - **Copyright:** © 2018 Mapbox (author Vladimir Agafonkin)
  - **Licence:** ISC
  - **Project:** https://github.com/mourner/delaunator-rs
- **robust**: adaptive-precision geometric predicates (used by delaunator).
  - **Copyright:** © the Spade developers (GeoRust)
  - **Licence:** MIT OR Apache-2.0
  - **Project:** https://github.com/georust/robust
- **i_overlay**: polygon boolean operations and offsetting
  (`geometry/boolean`, `geometry/path-ops`).
  - **Copyright:** © Nail Sharipov (iShape-Rust)
  - **Licence:** MIT OR Apache-2.0
  - **Project:** https://github.com/iShape-Rust/iOverlay
- **i_float**, **i_shape**, **i_tree**, **i_key_sort**: support crates of
  i_overlay.
  - **Copyright:** © Nail Sharipov (iShape-Rust)
  - **Licence:** MIT
  - **Project:** https://github.com/iShape-Rust
- **libm**: floating-point maths (used by i_float).
  - **Copyright:** © the Rust Project contributors
  - **Licence:** MIT
  - **Project:** https://github.com/rust-lang/compiler-builtins

### GIF export

- **gif**, with its **color_quant** NeuQuant quantiser and **weezl** LZW
  coder, used by `floret-video` to write animated GIFs for File → Export
  Video / GIF.
  - **Copyright:** © the image-rs developers (`gif`, `weezl`); © nwin and the
    image-rs developers (`color_quant`)
  - **Licence:** `gif` and `weezl`: MIT OR Apache-2.0; `color_quant`: MIT.
  - **Project:** https://github.com/image-rs/image-gif

### Audio decoding

- **symphonia** (and its `symphonia-*` codec and format crates): decodes
  audio files (WAV / PCM / ADPCM, FLAC, Ogg Vorbis and MP3) for the audio nodes.
  - **Copyright:** © Philip Deljanov and contributors
  - **Licence:** MPL-2.0 (a file-level licence). Floret uses the crates
    unmodified, so no MPL-covered file has been changed. The corresponding
    source is at https://github.com/pdeljanov/Symphonia and on crates.io.
  - **Project:** https://github.com/pdeljanov/Symphonia

### H.264 video

- **openh264** and **openh264-sys2** (Cisco's OpenH264 codec, compiled from
  the source the `-sys2` crate bundles) decode H.264 video files and encode
  MP4 exports in the **Windows and Linux builds only**.
  - **The macOS build does not include OpenH264.** On macOS, video plays and
    exports through Apple's AVFoundation / VideoToolbox, which macOS
    licenses for H.264. The crates are still listed in `Cargo.lock` and in
    `native/licenses/crates.md` because the lock file covers every platform.
  - **Copyright:** © Cisco Systems, Inc. (OpenH264); © Ralf Biedert and
    contributors (`openh264` crates)
  - **Licence:** BSD-2-Clause
  - **Project:** https://github.com/cisco/openh264 ·
    https://github.com/ralfbiedert/openh264-rs

BSD-2-Clause asks for the copyright notice and disclaimer in binary
distributions. Cisco's royalty-free H.264 patent coverage applies to the
prebuilt binaries Cisco distributes, not to OpenH264 compiled from source, as
here. Before releasing Windows or Linux builds, switch them to the OS's own
codecs (Windows Media Foundation, GStreamer / VA-API) or to Cisco's binary
downloaded at runtime, or check H.264 patent licensing.

---

## 5. Every other Rust crate

The interface toolkit (`egui`, `eframe`, `egui_dock`), the GPU layer (`wgpu`,
`naga`), text shaping and rasterising (`cosmic-text`, `swash`, `skrifa`,
`harfrust`), SVG (`resvg`), images (`image`, `png`), cameras (`nokhwa`), audio
I/O (`cpal`, `midir`), networking (`ureq`, `rustls`, `webpki-roots`), native
menus and dialogs (`muda`, `rfd`) and the hundreds of crates they pull in
are all open source under permissive licences (MIT, Apache-2.0, BSD, ISC,
Zlib, Unlicense, Unicode, CDLA-Permissive-2.0, Boost, MPL-2.0 for symphonia).
Where a crate offers a choice of licences, Floret uses it under the
permissive one. The complete list, with versions and licences, is in
[`native/licenses/crates.md`](native/licenses/crates.md).

A test (`licenses.rs` in `floret-app`) fails the build if `Cargo.lock` gains
a crate that is not in that list, or a crate under a licence outside the
allow-list.
