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
