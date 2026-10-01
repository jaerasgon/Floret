//! Tracker and camera sessions as the nodes drive them.

use floret_engine::devices::{CameraRequest, Devices, LandmarksRequest};
use floret_engine::image::Image;
use floret_engine::landmarks::Tracker;
use floret_input::InputDevices;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn gray(w: u32, h: u32) -> Arc<Image> {
    Arc::new(Image::new(w, h, vec![128; (w * h * 4) as usize]))
}

#[test]
fn tracker_sessions_load_track_and_stop() {
    let dev = InputDevices::default();
    let req = |image: Option<Arc<Image>>, start| LandmarksRequest {
        image,
        start,
        trackers: vec![Tracker::Hands, Tracker::Face, Tracker::Pose],
        count: 2,
    };
    // Stopped: nothing runs, nothing to report.
    assert_eq!(dev.landmarks("lm", &req(None, false)), Ok(None));
    assert_eq!(dev.trackers.active(), 0);

    // Started: the models load on the session thread, then each new frame
    // is tracked (an empty frame finds nobody).
    let deadline = Instant::now() + Duration::from_secs(120);
    let frame = gray(96, 64);
    let layers = loop {
        let r = dev
            .landmarks("lm", &req(Some(frame.clone()), true))
            .unwrap();
        if let Some(layers) = r.filter(|l| !l.is_empty()) {
            break layers;
        }
        assert!(Instant::now() < deadline, "tracker never produced a result");
        std::thread::sleep(Duration::from_millis(20));
    };
    let trackers: Vec<Tracker> = layers.iter().map(|l| l.tracker).collect();
    assert_eq!(trackers, [Tracker::Hands, Tracker::Face, Tracker::Pose]);
    assert!(layers.iter().all(|l| l.dets.is_empty() && l.ids.is_empty()));
    assert_eq!(dev.trackers.active(), 1);

    // A config change replaces the session; the falling edge ends it.
    let single = LandmarksRequest {
        trackers: vec![Tracker::Hands],
        ..req(None, true)
    };
    assert_eq!(dev.landmarks("lm", &single), Ok(None));
    assert_eq!(dev.landmarks("lm", &req(None, false)), Ok(None));
    assert_eq!(dev.trackers.active(), 0);
}

/// CI machines and this container have no camera: the node gets a clear
/// error instead of a picture. (Linux only: on macOS the first open asks
/// for camera permission, which nobody can answer on CI.)
#[cfg(target_os = "linux")]
#[test]
fn camera_without_a_device_reports_why() {
    if std::path::Path::new("/dev/video0").exists() {
        return;
    }
    let dev = InputDevices::default();
    let req = |start| CameraRequest {
        device: "",
        start,
        mirror: true,
    };
    let stopped = dev.camera("cam", &req(false)).unwrap();
    assert!(!stopped.ready && !stopped.image.is_ready());
    let deadline = Instant::now() + Duration::from_secs(20);
    let err = loop {
        match dev.camera("cam", &req(true)) {
            Err(e) => break e,
            Ok(f) => assert!(!f.ready),
        }
        assert!(Instant::now() < deadline, "no error reported");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(err, "No camera found.");
    // A named device that isn't there.
    let named = CameraRequest {
        device: "Studio Cam",
        start: true,
        mirror: false,
    };
    let err = loop {
        if let Err(e) = dev.camera("cam", &named) {
            break e;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(err.contains("Studio Cam"), "{err}");
    assert!(dev.cameras.devices().is_empty());
}
