//! Audio devices (cpal), owned by one thread: cpal streams aren't `Send`
//! everywhere, and opening a device (or the OS microphone prompt) can
//! block, so none of it happens on the UI thread.
//!
//! With no usable output device the thread renders the graph itself on a
//! timer, so analysers and the rest of the patch behave as if audio were
//! playing; it retries the device every few seconds.

use crate::render::{MicInput, Renderer};
use crate::system::{lock, DeviceInfo, Shared};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use floret_engine::audio::MicState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub enum DeviceCmd {
    Output(Option<String>),
    Input(Option<String>),
    Mic(bool),
    Refresh,
}

pub struct DeviceThread {
    tx: Sender<DeviceCmd>,
}

impl DeviceThread {
    pub fn spawn(shared: Arc<Shared>) -> DeviceThread {
        let (tx, rx) = channel();
        let spawned = std::thread::Builder::new()
            .name("floret audio devices".into())
            .spawn(move || run(shared, rx));
        if let Err(e) = spawned {
            eprintln!("floret: audio device thread failed to start: {e}");
        }
        DeviceThread { tx }
    }

    pub fn send(&self, cmd: DeviceCmd) {
        let _ = self.tx.send(cmd);
    }
}

/// A playing stream plus a flag its error callback raises.
struct Open {
    _stream: Stream,
    failed: Arc<AtomicBool>,
}

const RETRY: Duration = Duration::from_secs(3);

fn run(shared: Arc<Shared>, rx: Receiver<DeviceCmd>) {
    let host = cpal::default_host();
    refresh(&host, &shared);
    let (mut want_out, mut want_in) = (None::<String>, None::<String>);
    let mut mic_on = false;
    let mut out: Option<Open> = None;
    let mut mic: Option<Open> = None;
    let mut retry_at = Instant::now();
    let mut null = NullClock::new();
    loop {
        let wait = if out.is_some() {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(5)
        };
        let mut reopen_out = false;
        let mut reopen_mic = false;
        match rx.recv_timeout(wait) {
            Ok(DeviceCmd::Output(id)) => {
                want_out = id;
                reopen_out = true;
            }
            Ok(DeviceCmd::Input(id)) => {
                want_in = id;
                reopen_mic = mic_on;
            }
            Ok(DeviceCmd::Mic(on)) => {
                mic_on = on;
                reopen_mic = true;
            }
            Ok(DeviceCmd::Refresh) => refresh(&host, &shared),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if out
            .as_ref()
            .is_some_and(|o| o.failed.load(Ordering::Relaxed))
        {
            out = None;
            lock(&shared.status).error = Some("the audio device stopped".into());
            retry_at = Instant::now() + RETRY;
            refresh(&host, &shared);
        }
        if reopen_out {
            out = None;
            retry_at = Instant::now();
        }
        if out.is_none() && Instant::now() >= retry_at {
            let rate_before = lock(&shared.renderer()).sample_rate();
            match open_output(&host, want_out.as_deref(), &shared) {
                Ok((o, name)) => {
                    out = Some(o);
                    let mut st = lock(&shared.status);
                    st.output = Some(name);
                    st.error = None;
                }
                Err(e) => {
                    let mut st = lock(&shared.status);
                    st.output = None;
                    st.error = Some(e);
                    retry_at = Instant::now() + RETRY;
                }
            }
            null = NullClock::new();
            // A new renderer dropped the microphone feed; reconnect it.
            reopen_mic |= mic_on && lock(&shared.renderer()).sample_rate() != rate_before;
        }
        if mic
            .as_ref()
            .is_some_and(|m| m.failed.load(Ordering::Relaxed))
        {
            mic = None;
            lock(&shared.control).set_mic(None);
            shared.set_mic_state(MicState::Failed("the microphone stopped".into()));
        }
        if reopen_mic {
            mic = None;
            lock(&shared.control).set_mic(None);
            if mic_on {
                shared.set_mic_state(MicState::Starting);
                match open_mic(&host, want_in.as_deref(), &shared) {
                    Ok(m) => {
                        mic = Some(m);
                        shared.set_mic_state(MicState::Live);
                    }
                    Err(e) => shared.set_mic_state(MicState::Failed(e)),
                }
            } else {
                shared.set_mic_state(MicState::Off);
            }
        }
        if out.is_none() {
            null.run(&shared.renderer());
        }
    }
}

fn describe(d: &cpal::Device) -> Option<DeviceInfo> {
    let id = d.id().ok()?.to_string();
    let name = d
        .description()
        .map(|x| x.name().to_owned())
        .unwrap_or_else(|_| id.clone());
    Some(DeviceInfo { id, name })
}

fn refresh(host: &cpal::Host, shared: &Shared) {
    let list = |devs: Result<Vec<cpal::Device>, cpal::Error>| -> Vec<DeviceInfo> {
        devs.map(|v| v.iter().filter_map(describe).collect())
            .unwrap_or_default()
    };
    let outputs = list(host.output_devices().map(Iterator::collect));
    let inputs = list(host.input_devices().map(Iterator::collect));
    let mut st = lock(&shared.status);
    st.outputs = outputs;
    st.inputs = inputs;
}

fn find(
    host: &cpal::Host,
    id: Option<&str>,
    default: impl FnOnce() -> Option<cpal::Device>,
) -> Option<cpal::Device> {
    match id {
        Some(id) => id.parse().ok().and_then(|i| host.device_by_id(&i)),
        None => default(),
    }
}

fn open_output(
    host: &cpal::Host,
    id: Option<&str>,
    shared: &Shared,
) -> Result<(Open, String), String> {
    let device = find(host, id, || host.default_output_device())
        .ok_or_else(|| "no audio output device".to_owned())?;
    let name = describe(&device).map_or_else(|| "audio output".into(), |d| d.name);
    let supported = device
        .default_output_config()
        .map_err(|e| format!("{name}: {e}"))?;
    let renderer = shared.renderer_for(supported.sample_rate() as f32);
    let config = supported.config();
    let failed = Arc::new(AtomicBool::new(false));
    let f = failed.clone();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => output_stream::<f32>(&device, config, renderer, f),
        SampleFormat::F64 => output_stream::<f64>(&device, config, renderer, f),
        SampleFormat::I16 => output_stream::<i16>(&device, config, renderer, f),
        SampleFormat::I32 => output_stream::<i32>(&device, config, renderer, f),
        SampleFormat::U16 => output_stream::<u16>(&device, config, renderer, f),
        SampleFormat::U8 => output_stream::<u8>(&device, config, renderer, f),
        SampleFormat::I8 => output_stream::<i8>(&device, config, renderer, f),
        other => return Err(format!("{name}: unsupported sample format {other}")),
    }
    .map_err(|e| format!("{name}: {e}"))?;
    stream.play().map_err(|e| format!("{name}: {e}"))?;
    Ok((
        Open {
            _stream: stream,
            failed,
        },
        name,
    ))
}

fn fatal(e: &cpal::Error) -> bool {
    !matches!(
        e.kind(),
        cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied
    )
}

fn output_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    renderer: Arc<Mutex<Renderer>>,
    failed: Arc<AtomicBool>,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels).max(1);
    // Converted in chunks through a buffer allocated here, not on the
    // audio thread.
    let mut scratch = vec![0f32; 4096 * channels];
    device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            // Only contended while a device switch hands the renderer over.
            let Ok(mut r) = renderer.try_lock() else {
                data.fill(T::EQUILIBRIUM);
                return;
            };
            for chunk in data.chunks_mut(scratch.len()) {
                let s = &mut scratch[..chunk.len()];
                r.render(s, channels);
                for (d, x) in chunk.iter_mut().zip(s.iter()) {
                    *d = T::from_sample(*x);
                }
            }
        },
        move |e| {
            if fatal(&e) {
                failed.store(true, Ordering::Relaxed);
            }
        },
        None,
    )
}

fn open_mic(host: &cpal::Host, id: Option<&str>, shared: &Shared) -> Result<Open, String> {
    let device = find(host, id, || host.default_input_device())
        .ok_or_else(|| "no microphone found".to_owned())?;
    let name = describe(&device).map_or_else(|| "microphone".into(), |d| d.name);
    let supported = device.default_input_config().map_err(|e| {
        if e.kind() == cpal::ErrorKind::PermissionDenied {
            format!("microphone access was denied ({name})")
        } else {
            format!("{name}: {e}")
        }
    })?;
    let rate = supported.sample_rate() as f32;
    let (tx, rx) = rtrb::RingBuffer::new(rate as usize);
    let config = supported.config();
    let failed = Arc::new(AtomicBool::new(false));
    let f = failed.clone();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => input_stream::<f32>(&device, config, tx, f),
        SampleFormat::F64 => input_stream::<f64>(&device, config, tx, f),
        SampleFormat::I16 => input_stream::<i16>(&device, config, tx, f),
        SampleFormat::I32 => input_stream::<i32>(&device, config, tx, f),
        SampleFormat::U16 => input_stream::<u16>(&device, config, tx, f),
        SampleFormat::U8 => input_stream::<u8>(&device, config, tx, f),
        SampleFormat::I8 => input_stream::<i8>(&device, config, tx, f),
        other => return Err(format!("{name}: unsupported sample format {other}")),
    }
    .map_err(|e| match e.kind() {
        cpal::ErrorKind::PermissionDenied => format!("microphone access was denied ({name})"),
        _ => format!("{name}: {e}"),
    })?;
    stream.play().map_err(|e| format!("{name}: {e}"))?;
    let out_rate = lock(&shared.renderer()).sample_rate();
    let feed = MicInput::new(rx, rate, out_rate);
    lock(&shared.control).set_mic(Some(Box::new(feed)));
    Ok(Open {
        _stream: stream,
        failed,
    })
}

fn input_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut tx: rtrb::Producer<f32>,
    failed: Arc<AtomicBool>,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels).max(1);
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            for frame in data.chunks(channels) {
                let sum: f32 = frame.iter().map(|s| f32::from_sample(*s)).sum();
                // A full ring means the output side stalled: drop input.
                let _ = tx.push(sum / frame.len() as f32);
            }
        },
        move |e| {
            if fatal(&e) {
                failed.store(true, Ordering::Relaxed);
            }
        },
        None,
    )
}

/// Renders in real time with no device, so the graph keeps running.
struct NullClock {
    start: Instant,
    done: u64,
    buf: Vec<f32>,
}

impl NullClock {
    fn new() -> NullClock {
        NullClock {
            start: Instant::now(),
            done: 0,
            buf: vec![0.0; 2048],
        }
    }

    fn run(&mut self, renderer: &Arc<Mutex<Renderer>>) {
        let mut r = lock(renderer);
        let rate = f64::from(r.sample_rate());
        let due = (self.start.elapsed().as_secs_f64() * rate) as u64;
        // Fell far behind (suspended?): skip ahead rather than catch up.
        if due.saturating_sub(self.done) > rate as u64 / 4 {
            self.done = due.saturating_sub(1024);
        }
        while self.done < due {
            let n = ((due - self.done) as usize).min(self.buf.len() / 2);
            r.render(&mut self.buf[..n * 2], 2);
            self.done += n as u64;
        }
    }
}
