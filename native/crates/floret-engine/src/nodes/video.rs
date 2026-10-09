//! `video/*` nodes: video files as images. New in the native app (the web
//! app had none), so there's no TS reference.
//!
//! Both read the app's video provider (`crate::video`), which owns the
//! playhead and the decoder; the node only gathers the request.
//!
//! The source can also be an image sequence (numbered PNG / JPEG frames:
//! a folder, one frame of the run, a `name_####.png` pattern, or a list
//! of frames one per line), played at the `fps` pin's rate.

use super::shared::first;
use super::Registrar;
use crate::node::{raw, EvalCtx, NodeImpl, NodeInit, NodeResult};
use crate::value::{one, Spread, Value};
use crate::video::{self, Seek, VideoFrame, VideoRequest};

pub fn register(r: &mut Registrar) {
    r.node("video/file", file);
    r.node("video/frame-at", frame_at);
}

/// The pin's source string (shared, not copied: it can be a long data URL).
fn source(v: &Value) -> &str {
    match v {
        Value::Str(s) => s.trim(),
        _ => "",
    }
}

/// A wired number; unwired pins (default `null`) and empty spreads are `None`.
fn wired_num(v: &Value) -> Option<f64> {
    match v {
        Value::Num(n) if n.is_finite() => Some(*n),
        Value::Bool(_) | Value::Str(_) => Some(v.to_number()).filter(|n| n.is_finite()),
        _ => None,
    }
}

fn bool_or(v: &Value, default: bool) -> bool {
    match v {
        Value::Undefined | Value::Null => default,
        v => v.truthy(),
    }
}

fn request(cx: &EvalCtx, req: VideoRequest) -> Result<VideoFrame, String> {
    if req.source.is_empty() {
        return Ok(VideoFrame::empty());
    }
    let videos = video::installed().ok_or("video playback isn't available")?;
    videos.video(cx.node_id, &req)
}

fn num(n: f64) -> Spread {
    one(Value::Num(n))
}

/// `[image, …, width, height, ready]` with `extra` after the image.
fn outputs(f: VideoFrame, extra: &[f64]) -> NodeResult {
    let (w, h) = if f.ready {
        (f64::from(f.image.width()), f64::from(f.image.height()))
    } else {
        (0.0, 0.0)
    };
    let mut out = vec![one(Value::Image(f.image))];
    out.extend(extra.iter().map(|&n| num(n)));
    out.extend([num(w), num(h), one(Value::Bool(f.ready))]);
    Ok(out)
}

/// An image sequence's frame rate (the `fps` pin; 30 unwired).
fn fps(v: &Value) -> f64 {
    wired_num(v).filter(|n| *n > 0.0).unwrap_or(DEFAULT_FPS)
}

/// Frames a second for image sequences unless the `fps` pin says.
pub const DEFAULT_FPS: f64 = 30.0;

/// Inputs: url, play, loop, speed, scrub, fps. Outputs: image, time,
/// duration, width, height, ready.
fn file(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let url = first(inputs, 0);
        let src = source(&url);
        let speed = wired_num(&first(inputs, 3)).unwrap_or(1.0);
        let req = VideoRequest {
            source: src,
            play: bool_or(&first(inputs, 1), true),
            looping: bool_or(&first(inputs, 2), true),
            speed,
            seek: wired_num(&first(inputs, 4)).map(Seek::Position),
            dt: cx.tick_frame.dt,
            restart: cx.tick_frame.frame_count == 0.0,
            wait: video::waiting(),
            fps: fps(&first(inputs, 5)),
        };
        let f = request(cx, req)?;
        let (time, duration) = (f.time, f.duration);
        outputs(f, &[time, duration])
    })
}

/// Inputs: url, time (seconds), fps. Outputs: image, duration, width,
/// height, ready.
fn frame_at(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs: &[Spread], cx| {
        let url = first(inputs, 0);
        let src = source(&url);
        let time = wired_num(&first(inputs, 1)).unwrap_or(0.0);
        let req = VideoRequest {
            source: src,
            play: false,
            looping: false,
            speed: 1.0,
            seek: Some(Seek::Seconds(time)),
            dt: cx.tick_frame.dt,
            restart: false,
            wait: video::waiting(),
            fps: fps(&first(inputs, 2)),
        };
        let f = request(cx, req)?;
        let duration = f.duration;
        outputs(f, &[duration])
    })
}
