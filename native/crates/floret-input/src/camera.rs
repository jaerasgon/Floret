//! Camera capture (`camera-registry.ts`) on nokhwa: AVFoundation on macOS,
//! Media Foundation on Windows, V4L2 on Linux.
//!
//! Each `input/camera` node that is started gets a capture thread that
//! keeps only the latest frame, converted to RGBA (and mirrored if asked).
//! Like `getUserMedia`'s default the camera opens at about 640×480, 30 fps.
//! The camera turns off when `start` falls, the device changes, or the
//! node stops polling (deleted, patch switched).

use floret_engine::devices::{CameraFrame, CameraRequest};
use floret_engine::image::Image;
use nokhwa::utils::{
    ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
    Resolution,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// A session its node stopped polling shuts down.
const IDLE: Duration = Duration::from_secs(2);
/// How long a camera that has started may take to send its first picture
/// before it counts as dead.
const FIRST_PICTURE: Duration = Duration::from_secs(4);
/// How often the device list may be re-queried.
const RESCAN: Duration = Duration::from_secs(3);
/// Frame formats `convert` understands.
const FORMATS: &[FrameFormat] = &[
    FrameFormat::MJPEG,
    FrameFormat::YUYV,
    FrameFormat::NV12,
    FrameFormat::RAWRGB,
    FrameFormat::RAWBGR,
    FrameFormat::GRAY,
];

// ─── Frame conversion ────────────────────────────────────────────────────────

/// BT.601 limited-range YUV → RGB.
fn yuv(y: u8, u: u8, v: u8) -> [u8; 4] {
    let c = i32::from(y) - 16;
    let d = i32::from(u) - 128;
    let e = i32::from(v) - 128;
    let ch = |x: i32| ((x + 128) >> 8).clamp(0, 255) as u8;
    [
        ch(298 * c + 409 * e),
        ch(298 * c - 100 * d - 208 * e),
        ch(298 * c + 516 * d),
        255,
    ]
}

/// A captured frame as straight RGBA8, flipped horizontally if `mirror`.
pub fn convert(
    format: FrameFormat,
    width: u32,
    height: u32,
    data: &[u8],
    mirror: bool,
) -> Result<Image, String> {
    let (w, h) = (width as usize, height as usize);
    let short = || format!("{format} frame too short");
    let mut rgba = match format {
        FrameFormat::MJPEG => {
            let img = image::load_from_memory_with_format(data, image::ImageFormat::Jpeg)
                .map_err(|e| format!("bad MJPEG frame: {e}"))?
                .into_rgba8();
            if (img.width(), img.height()) != (width, height) {
                return Ok(finish(img.width(), img.height(), img.into_raw(), mirror));
            }
            img.into_raw()
        }
        FrameFormat::YUYV => {
            let src = data.get(..w * h * 2).ok_or_else(short)?;
            let mut out = Vec::with_capacity(w * h * 4);
            for q in src.as_chunks::<4>().0 {
                out.extend_from_slice(&yuv(q[0], q[1], q[3]));
                out.extend_from_slice(&yuv(q[2], q[1], q[3]));
            }
            out
        }
        FrameFormat::NV12 => {
            let luma = data.get(..w * h).ok_or_else(short)?;
            let chroma = data
                .get(w * h..w * h + w.div_ceil(2) * 2 * h.div_ceil(2))
                .ok_or_else(short)?;
            let cw = w.div_ceil(2) * 2;
            let mut out = Vec::with_capacity(w * h * 4);
            for y in 0..h {
                for x in 0..w {
                    let c = (y / 2) * cw + (x / 2) * 2;
                    out.extend_from_slice(&yuv(luma[y * w + x], chroma[c], chroma[c + 1]));
                }
            }
            out
        }
        FrameFormat::RAWRGB | FrameFormat::RAWBGR => {
            let src = data.get(..w * h * 3).ok_or_else(short)?;
            let bgr = format == FrameFormat::RAWBGR;
            src.as_chunks::<3>()
                .0
                .iter()
                .flat_map(|p| {
                    if bgr {
                        [p[2], p[1], p[0], 255]
                    } else {
                        [p[0], p[1], p[2], 255]
                    }
                })
                .collect()
        }
        FrameFormat::GRAY => {
            let src = data.get(..w * h).ok_or_else(short)?;
            src.iter().flat_map(|&g| [g, g, g, 255]).collect()
        }
    };
    if mirror {
        mirror_rows(&mut rgba, w);
    }
    Ok(Image::new(width, height, rgba))
}

fn finish(w: u32, h: u32, mut rgba: Vec<u8>, mirror: bool) -> Image {
    if mirror {
        mirror_rows(&mut rgba, w as usize);
    }
    Image::new(w, h, rgba)
}

fn mirror_rows(rgba: &mut [u8], w: usize) {
    for row in rgba.chunks_exact_mut(w * 4) {
        for x in 0..w / 2 {
            let (a, b) = (x * 4, (w - 1 - x) * 4);
            for k in 0..4 {
                row.swap(a + k, b + k);
            }
        }
    }
}

// ─── Devices ─────────────────────────────────────────────────────────────────

#[derive(Default)]
struct DeviceList {
    /// Display names (unique), with their nokhwa indices.
    cameras: Vec<(String, CameraIndex)>,
    scanned: Option<Instant>,
    scanning: bool,
}

fn scan() -> Vec<(String, CameraIndex)> {
    let infos = nokhwa::query(ApiBackend::Auto).unwrap_or_default();
    let names = crate::midi::unique_names(infos.iter().map(|i| i.human_name()));
    let mut cams: Vec<_> = names
        .into_iter()
        .zip(infos.iter().map(|i| (i.index().clone(), i.description())))
        .map(|(name, (index, description))| (name, index, built_in(description)))
        .collect();
    // The built-in camera first (so it is the default): with a display, a
    // phone or a virtual camera connected, the first one listed may never
    // send a picture. The sort is stable, so the rest keep their order.
    cams.sort_by_key(|(_, _, built_in)| !built_in);
    cams.into_iter().map(|(n, i, _)| (n, i)).collect()
}

/// Whether a camera's description names it a built-in one. On macOS it holds
/// the AVFoundation device type (`…BuiltInWideAngleCamera`, or `…External`
/// for displays, USB and phone cameras); other systems never say so.
fn built_in(description: &str) -> bool {
    description.contains("BuiltIn")
}

// ─── Capture sessions ────────────────────────────────────────────────────────

#[derive(Default)]
struct Latest {
    image: Option<Arc<Image>>,
    error: Option<String>,
    last_poll: Option<Instant>,
    done: bool,
    /// The camera in use, once chosen.
    camera: Option<String>,
    /// When the camera started streaming; no picture yet means it may be dead.
    streaming_since: Option<Instant>,
}

impl Latest {
    /// A camera that started streaming but has sent nothing for too long.
    fn stalled(&self) -> bool {
        self.image.is_none()
            && self.error.is_none()
            && self
                .streaming_since
                .is_some_and(|t| t.elapsed() >= FIRST_PICTURE)
    }
}

struct Shared {
    latest: Mutex<Latest>,
    mirror: AtomicBool,
    cancel: AtomicBool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Latest> {
        self.latest.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn idle(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
            || self.lock().last_poll.is_none_or(|t| t.elapsed() >= IDLE)
    }

    fn fail(&self, msg: String) {
        let mut l = self.lock();
        l.error = Some(msg);
        l.done = true;
    }
}

struct Session {
    device: String,
    shared: Arc<Shared>,
    /// Cameras that were tried and sent nothing (default camera only).
    skipped: Vec<String>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shared.cancel.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub struct Cameras {
    list: Arc<Mutex<DeviceList>>,
    sessions: Mutex<HashMap<String, Session>>,
}

impl Cameras {
    /// Connected cameras' names (the `device` param's values). Cached; a
    /// rescan runs in the background when the list is older than 3 s.
    pub fn devices(&self) -> Vec<String> {
        let mut list = self.list.lock().unwrap_or_else(|e| e.into_inner());
        if !list.scanning && list.scanned.is_none_or(|t| t.elapsed() >= RESCAN) {
            list.scanning = true;
            let shared = self.list.clone();
            let spawned = std::thread::Builder::new()
                .name("floret-camera-scan".into())
                .spawn(move || {
                    let cams = scan();
                    let mut l = shared.lock().unwrap_or_else(|e| e.into_inner());
                    (l.cameras, l.scanned, l.scanning) = (cams, Some(Instant::now()), false);
                });
            if spawned.is_err() {
                list.scanning = false;
            }
        }
        list.cameras.iter().map(|(n, _)| n.clone()).collect()
    }

    /// The node's tick (`cameraRegistry.ensure` + `isReady`).
    pub fn request(&self, node: &str, req: &CameraRequest) -> Result<CameraFrame, String> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        sessions.retain(|_, s| {
            let l = s.shared.lock();
            (!l.done || l.error.is_some()) && l.last_poll.is_some_and(|t| t.elapsed() < IDLE)
        });
        let idle = || CameraFrame {
            image: Image::placeholder(),
            ready: false,
        };
        if !req.start {
            sessions.remove(node);
            return Ok(idle());
        }
        if sessions.get(node).is_some_and(|s| s.device != req.device) {
            sessions.remove(node);
        }
        let stalled = sessions
            .get(node)
            .and_then(|s| s.shared.lock().stalled().then(|| s.shared.clone()));
        if let Some(shared) = stalled {
            // Take the dead camera out, and with the default camera try the
            // next one; a camera picked by name just gets a warning.
            let name = shared.lock().camera.clone().unwrap_or_default();
            let mut skipped = sessions
                .remove(node)
                .map(|s| s.skipped.clone())
                .unwrap_or_default();
            skipped.push(name.clone());
            if req.device.is_empty() {
                sessions.insert(node.to_owned(), self.spawn(req.device, req.mirror, skipped));
            } else {
                shared.fail(not_sending(&name));
                sessions.insert(
                    node.to_owned(),
                    Session {
                        device: req.device.to_owned(),
                        shared,
                        skipped,
                    },
                );
            }
        }
        let s = sessions
            .entry(node.to_owned())
            .or_insert_with(|| self.spawn(req.device, req.mirror, Vec::new()));
        s.shared.mirror.store(req.mirror, Ordering::Relaxed);
        let mut l = s.shared.lock();
        l.last_poll = Some(Instant::now());
        if let Some(e) = &l.error {
            return Err(e.clone());
        }
        Ok(match &l.image {
            Some(img) => CameraFrame {
                image: img.clone(),
                ready: true,
            },
            None => idle(),
        })
    }

    fn spawn(&self, device: &str, mirror: bool, skipped: Vec<String>) -> Session {
        let shared = Arc::new(Shared {
            latest: Mutex::new(Latest {
                last_poll: Some(Instant::now()),
                ..Latest::default()
            }),
            mirror: AtomicBool::new(mirror),
            cancel: AtomicBool::new(false),
        });
        let s = shared.clone();
        let dev = device.to_owned();
        let list = self.list.clone();
        let skip = skipped.clone();
        let spawned = std::thread::Builder::new()
            .name("floret-camera".into())
            .spawn(move || capture(&s, &dev, &list, &skip));
        if let Err(e) = spawned {
            shared.fail(format!("couldn't start the camera: {e}"));
        }
        Session {
            device: device.to_owned(),
            shared,
            skipped,
        }
    }
}

/// macOS asks the user once; the app's Info.plist explains why (see the
/// floret-app build script).
#[cfg(target_os = "macos")]
fn permission() -> Result<(), String> {
    if nokhwa::nokhwa_check() {
        return Ok(());
    }
    let answer = Arc::new((Mutex::new(None::<bool>), std::sync::Condvar::new()));
    let a = answer.clone();
    nokhwa::nokhwa_initialize(move |granted| {
        *a.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(granted);
        a.1.notify_all();
    });
    let (lock, cv) = &*answer;
    let got = cv
        .wait_timeout_while(
            lock.lock().unwrap_or_else(|e| e.into_inner()),
            Duration::from_secs(120),
            |g| g.is_none(),
        )
        .map(|(g, _)| *g)
        .unwrap_or(None);
    match got {
        Some(true) => Ok(()),
        _ => Err("Camera access was denied. Allow Floret in System Settings › Privacy & Security › Camera.".into()),
    }
}

#[cfg(not(target_os = "macos"))]
fn permission() -> Result<(), String> {
    Ok(())
}

/// Formats to try, best first: sizes nearest 640×480, then MJPEG and the raw
/// formats, then frame rates from 30 down. NV12 comes last: on macOS nokhwa
/// asks for a 10-bit variant of it.
fn candidates() -> Vec<CameraFormat> {
    const SIZES: &[(u32, u32)] = &[
        (640, 480),
        (640, 360),
        (800, 600),
        (848, 480),
        (960, 540),
        (1024, 576),
        (1280, 720),
        (1280, 960),
        (1920, 1080),
        (352, 288),
        (320, 240),
    ];
    const RATES: &[u32] = &[30, 60, 25, 24, 20, 15, 10, 5];
    let dist = |(w, h): (u32, u32)| {
        let (dx, dy) = (i64::from(w) - 640, i64::from(h) - 480);
        dx * dx + dy * dy
    };
    let mut sizes = SIZES.to_vec();
    sizes.sort_by_key(|s| dist(*s));
    let mut out = Vec::new();
    for (w, h) in sizes {
        for format in FORMATS {
            for fps in RATES {
                out.push(CameraFormat::new(Resolution::new(w, h), *format, *fps));
            }
        }
    }
    out.sort_by_key(|f| f.format() == FrameFormat::NV12);
    out
}

/// Opens the camera at the best format it accepts. nokhwa's `Closest` only
/// looks at one frame format, and on a Mac some listed formats are refused
/// when set, so each candidate is tried until one is accepted.
fn open(index: &CameraIndex) -> Result<nokhwa::Camera, String> {
    let want = CameraFormat::new(Resolution::new(640, 480), FrameFormat::MJPEG, 30);
    let closest = RequestedFormat::with_formats(RequestedFormatType::Closest(want), FORMATS);
    let mut last = match nokhwa::Camera::new(index.clone(), closest) {
        Ok(c) => return Ok(c),
        Err(e) => e,
    };
    for format in candidates() {
        let exact = RequestedFormat::with_formats(RequestedFormatType::Exact(format), FORMATS);
        match nokhwa::Camera::new(index.clone(), exact) {
            Ok(c) => return Ok(c),
            // Unknown to this camera: not worth reporting.
            Err(nokhwa::NokhwaError::OpenDeviceError(..)) => {}
            Err(e) => last = e,
        }
    }
    Err(last.to_string())
}

fn not_sending(name: &str) -> String {
    format!(
        "Camera “{name}” isn't sending pictures. Pick another camera in the device menu, or check it isn't in use."
    )
}

fn capture(shared: &Shared, device: &str, list: &Mutex<DeviceList>, skipped: &[String]) {
    if let Err(e) = permission() {
        return shared.fail(e);
    }
    let cams = scan();
    {
        let mut l = list.lock().unwrap_or_else(|e| e.into_inner());
        (l.cameras, l.scanned) = (cams.clone(), Some(Instant::now()));
    }
    let (name, index) = if device.is_empty() {
        match cams.iter().find(|(n, _)| !skipped.contains(n)) {
            Some((n, i)) => (n.clone(), i.clone()),
            None if skipped.is_empty() => return shared.fail("No camera found.".into()),
            None => return shared.fail(not_sending(&skipped.join("”, “"))),
        }
    } else {
        match cams.iter().find(|(n, _)| n == device) {
            Some((n, i)) => (n.clone(), i.clone()),
            None => return shared.fail(format!("Camera “{device}” isn't connected.")),
        }
    };
    shared.lock().camera = Some(name);
    let mut cam = match open(&index) {
        Ok(c) => c,
        Err(e) => return shared.fail(format!("Couldn't open the camera: {e}")),
    };
    if let Err(e) = cam.open_stream() {
        return shared.fail(format!("Couldn't start the camera: {e}"));
    }
    shared.lock().streaming_since = Some(Instant::now());
    let mut failures = 0;
    while !shared.idle() {
        match cam.frame() {
            Ok(buf) => {
                let res = buf.resolution();
                let mirror = shared.mirror.load(Ordering::Relaxed);
                match convert(
                    buf.source_frame_format(),
                    res.width(),
                    res.height(),
                    buf.buffer(),
                    mirror,
                ) {
                    Ok(img) => {
                        failures = 0;
                        shared.lock().image = Some(Arc::new(img));
                    }
                    Err(_) => failures += 1,
                }
            }
            Err(e) => {
                failures += 1;
                if failures > 60 {
                    let _ = cam.stop_stream();
                    return shared.fail(format!("The camera stopped: {e}"));
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        }
    }
    let _ = cam.stop_stream();
    shared.lock().done = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_raw_formats_and_mirrors() {
        // 2×1 frames.
        let rgb = convert(FrameFormat::RAWRGB, 2, 1, &[1, 2, 3, 4, 5, 6], false).unwrap();
        assert_eq!(rgb.rgba(), [1, 2, 3, 255, 4, 5, 6, 255]);
        let bgr = convert(FrameFormat::RAWBGR, 2, 1, &[1, 2, 3, 4, 5, 6], true).unwrap();
        assert_eq!(bgr.rgba(), [6, 5, 4, 255, 3, 2, 1, 255]);
        let gray = convert(FrameFormat::GRAY, 2, 1, &[0, 200], false).unwrap();
        assert_eq!(gray.rgba(), [0, 0, 0, 255, 200, 200, 200, 255]);
        // YUYV: black then white, neutral chroma.
        let yuyv = convert(FrameFormat::YUYV, 2, 1, &[16, 128, 235, 128], false).unwrap();
        assert_eq!(yuyv.rgba(), [0, 0, 0, 255, 255, 255, 255, 255]);
        // NV12 2×2: luma plane then one interleaved UV pair (pure red-ish).
        let nv12 = convert(FrameFormat::NV12, 2, 2, &[81, 81, 81, 81, 90, 240], false).unwrap();
        let px = nv12.pixel(1, 1);
        assert!(px[0] > 240 && px[1] < 10 && px[2] < 10, "{px:?}");
        assert!(convert(FrameFormat::YUYV, 4, 4, &[0; 8], false).is_err());
    }

    #[test]
    fn built_in_cameras_are_told_from_the_rest() {
        assert!(built_in(
            "Apple Inc.: UniqueID - AVCaptureDeviceTypeBuiltInWideAngleCamera, Front f2.4"
        ));
        assert!(!built_in(
            "Apple Inc.: Studio Display - AVCaptureDeviceTypeExternal, Unspecified f0"
        ));
        assert!(!built_in("USB Camera"));
    }

    #[test]
    fn a_camera_with_no_picture_after_a_while_is_stalled() {
        let mut l = Latest::default();
        assert!(!l.stalled(), "not started yet");
        l.streaming_since = Some(Instant::now());
        assert!(!l.stalled(), "just started");
        l.streaming_since = Instant::now().checked_sub(FIRST_PICTURE + Duration::from_secs(1));
        assert!(l.stalled());
        l.image = Some(Arc::new(Image::new(1, 1, vec![0, 0, 0, 255])));
        assert!(!l.stalled(), "a picture arrived");
        l.image = None;
        l.error = Some("x".into());
        assert!(!l.stalled(), "already failed");
    }

    #[test]
    fn candidates_start_at_vga_and_cover_cameras_without_mjpeg() {
        let all = candidates();
        let f = |w, h, fmt, fps| CameraFormat::new(Resolution::new(w, h), fmt, fps);
        assert_eq!(all[0], f(640, 480, FrameFormat::MJPEG, 30));
        assert!(all.contains(&f(1280, 720, FrameFormat::YUYV, 30)));
        // NV12 only after every other format.
        let first_nv12 = all.iter().position(|c| c.format() == FrameFormat::NV12);
        assert!(all[..first_nv12.unwrap()]
            .iter()
            .all(|c| c.format() != FrameFormat::NV12));
        // Nearer sizes come before farther ones.
        let at = |w| all.iter().position(|c| c.resolution().width() == w);
        assert!(at(1280) < at(1920));
    }

    #[test]
    fn decodes_mjpeg() {
        let mut jpeg = Vec::new();
        let img = image::RgbImage::from_fn(8, 4, |x, _| {
            if x < 4 {
                image::Rgb([250, 0, 0])
            } else {
                image::Rgb([0, 0, 250])
            }
        });
        image::DynamicImage::ImageRgb8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut jpeg),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        let out = convert(FrameFormat::MJPEG, 8, 4, &jpeg, true).unwrap();
        assert_eq!((out.width(), out.height()), (8, 4));
        // Mirrored: blue on the left.
        let (l, r) = (out.pixel(0, 0), out.pixel(7, 0));
        assert!(l[2] > 200 && r[0] > 200, "{l:?} {r:?}");
        assert!(convert(FrameFormat::MJPEG, 8, 4, b"not a jpeg", false).is_err());
    }
}
