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
