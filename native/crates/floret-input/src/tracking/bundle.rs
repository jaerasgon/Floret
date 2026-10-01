//! MediaPipe `.task` bundles: uncompressed ZIP archives of `.tflite` files.
//!
//! The bundles are the ones the web app self-hosts under
//! `public/mediapipe/models/` (Apache-2.0, see the NOTICE there); the
//! native build embeds the same files, so both apps track with identical
//! models and the binary works offline.

use std::sync::{Arc, OnceLock};

pub static HAND_TASK: &[u8] =
    include_bytes!("../../../../../public/mediapipe/models/hand_landmarker.task");
pub static FACE_TASK: &[u8] =
    include_bytes!("../../../../../public/mediapipe/models/face_landmarker.task");
pub static POSE_TASK: &[u8] =
    include_bytes!("../../../../../public/mediapipe/models/pose_landmarker_lite.task");

fn u16_at(b: &[u8], p: usize) -> Option<usize> {
    Some(usize::from(u16::from_le_bytes(
        b.get(p..p + 2)?.try_into().ok()?,
    )))
}

fn u32_at(b: &[u8], p: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(p..p + 4)?.try_into().ok()?) as usize)
}

/// The bytes of `name` inside a stored (uncompressed) ZIP.
pub fn entry<'a>(zip: &'a [u8], name: &str) -> Result<&'a [u8], String> {
    let missing = || format!("{name} not found in model bundle");
    // End of central directory: the last `PK\x05\x06` (comment ≤ 64 KB).
    let lo = zip.len().saturating_sub(22 + 65_535);
    let eocd = (lo..zip.len().saturating_sub(21))
        .rev()
        .find(|&p| zip[p..p + 4] == [0x50, 0x4b, 0x05, 0x06])
        .ok_or("model bundle is not a zip archive")?;
    let count = u16_at(zip, eocd + 10).ok_or_else(missing)?;
    let mut p = u32_at(zip, eocd + 16).ok_or_else(missing)?;
    for _ in 0..count {
        if u32_at(zip, p) != Some(0x0201_4b50) {
            break;
        }
        let method = u16_at(zip, p + 10).ok_or_else(missing)?;
        let size = u32_at(zip, p + 20).ok_or_else(missing)?;
        let (n, e, c) = (
            u16_at(zip, p + 28).ok_or_else(missing)?,
            u16_at(zip, p + 30).ok_or_else(missing)?,
            u16_at(zip, p + 32).ok_or_else(missing)?,
        );
        let local = u32_at(zip, p + 42).ok_or_else(missing)?;
        if zip.get(p + 46..p + 46 + n) == Some(name.as_bytes()) {
            if method != 0 {
                return Err(format!(
                    "{name} is compressed; only stored entries are supported"
                ));
            }
            let ln = u16_at(zip, local + 26).ok_or_else(missing)?;
            let le = u16_at(zip, local + 28).ok_or_else(missing)?;
            let start = local + 30 + ln + le;
            return zip.get(start..start + size).ok_or_else(missing);
        }
        p += 46 + n + e + c;
    }
    Err(missing())
}

/// The models a tracker needs, parsed once per process.
pub struct Models {
    pub detector: Arc<crate::nn::Model>,
    pub landmarks: Arc<crate::nn::Model>,
}

fn load(task: &[u8], detector: &str, landmarks: &str) -> Result<Models, String> {
    let model = |name| {
        crate::nn::Model::from_bytes(entry(task, name)?)
            .map(Arc::new)
            .map_err(|e| format!("{name}: {e}"))
    };
    Ok(Models {
        detector: model(detector)?,
        landmarks: model(landmarks)?,
    })
}

type Cell = OnceLock<Result<Arc<Models>, String>>;

fn cached(cell: &'static Cell, task: &[u8], det: &str, lm: &str) -> Result<Arc<Models>, String> {
    cell.get_or_init(|| load(task, det, lm).map(Arc::new))
        .clone()
}

pub fn hand() -> Result<Arc<Models>, String> {
    static C: Cell = OnceLock::new();
    cached(
        &C,
        HAND_TASK,
        "hand_detector.tflite",
        "hand_landmarks_detector.tflite",
    )
}

pub fn face() -> Result<Arc<Models>, String> {
    static C: Cell = OnceLock::new();
    cached(
        &C,
        FACE_TASK,
        "face_detector.tflite",
        "face_landmarks_detector.tflite",
    )
}

pub fn pose() -> Result<Arc<Models>, String> {
    static C: Cell = OnceLock::new();
    cached(
        &C,
        POSE_TASK,
        "pose_detector.tflite",
        "pose_landmarks_detector.tflite",
    )
}
