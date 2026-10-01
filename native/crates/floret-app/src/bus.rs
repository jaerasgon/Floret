//! The Send / Receive bus host: the engine's channel store, the renderer
//! that turns layer sends into PNGs, and an optional HTTP server with the
//! dev-server relay's API (`floret-bus.vite.mjs`), so the Raspberry Pi
//! e-ink poller (`tools/eink/floret_eink.py`) works unchanged:
//!
//! ```text
//! POST /__floret/bus/<channel>        store payload (any content type); bump version
//! GET  /__floret/bus/<channel>        the latest payload, with its content type
//! GET  /__floret/bus/<channel>/poll   { version, has, contentType }
//! ```
//!
//! The server is off by default (it listens on the LAN); the channel store
//! is always installed, so Send → Receive works inside the app regardless.

use crate::theme;
use eframe::egui::{self, RichText};
use eframe::egui_wgpu;
use floret_engine::bus::{Bus, BusMessage, MemoryBus};
use floret_engine::scene::Paint;
use floret_render::{RenderClock, Renderer};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The relay's port in the web app (Vite's dev server), which the Pi
/// script is configured with.
pub const PORT: u16 = 5173;
const PREFIX: &str = "/__floret/bus/";
const MAX_HEADER: usize = 64 * 1024;
const MAX_BODY: usize = 64 * 1024 * 1024;

pub struct BusHost {
    store: Arc<MemoryBus>,
    server: Option<Server>,
    /// Why the server isn't running, when starting it failed.
    error: Option<String>,
    /// Rasterises layer sends (created on first use).
    renderer: Option<Renderer>,
}

struct Server {
    addr: SocketAddr,
    /// Where the LAN reaches us (shown in the status bar).
    lan: IpAddr,
    stop: Arc<AtomicBool>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl BusHost {
    /// Create the store and install it as the engine's bus.
    pub fn new() -> BusHost {
        let store = Arc::new(MemoryBus::new());
        floret_engine::bus::install(Some(store.clone()));
        BusHost {
            store,
            server: None,
            error: None,
            renderer: None,
        }
    }

    /// Rasterise pending layer sends and publish them as PNGs.
    pub fn process(&mut self, rs: Option<&egui_wgpu::RenderState>, clock: RenderClock) {
        let jobs = self.store.take_layer_jobs();
        let Some(rs) = rs else {
            return;
        };
        for job in jobs {
            let renderer = self.renderer.get_or_insert_with(|| {
                Renderer::new(&rs.device, &rs.queue, floret_render::OFFSCREEN_FORMAT, 1)
            });
            renderer.set_clock(clock);
            let (w, h) = (
                job.width.clamp(1, floret_render::MAX_CANVAS),
                job.height.clamp(1, floret_render::MAX_CANVAS),
            );
            let (rgba, _) = floret_render::render_and_read(
                &rs.device,
                &rs.queue,
                renderer,
                &job.layers,
                (w, h),
                Paint::from_color(job.background).rgba,
            );
            if let Some(png) = encode_png(&rgba, w, h) {
                self.store.publish(&job.channel, "image/png", png);
            }
        }
    }

    fn set_serving(&mut self, on: bool) {
        if !on {
            self.server = None;
            return;
        }
        match Server::start(self.store.clone(), PORT) {
            Ok(s) => {
                self.server = Some(s);
                self.error = None;
            }
            Err(e) => self.error = Some(format!("Can't listen on port {PORT}: {e}")),
        }
    }

    /// Status-bar control: the server toggle and its LAN address.
    pub fn status(&mut self, ui: &mut egui::Ui) {
        let mut on = self.server.is_some();
        let label = match &self.server {
            Some(s) => format!("Bus · http://{}:{}", s.lan, s.addr.port()),
            None => "Bus off".to_owned(),
        };
        let r = ui
            .toggle_value(&mut on, RichText::new(label).small())
            .on_hover_text(
                "Serve the Send / Receive bus on the local network \
                 (/__floret/bus/<channel>) — e.g. for the e-ink poller.",
            );
        if r.changed() {
            self.set_serving(on);
        }
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(theme::DANGER).small());
        }
    }
}

fn encode_png(rgba: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().ok()?.write_image_data(rgba).ok()?;
    Some(out)
}

/// This machine's LAN address: the local end of a UDP "connection" to a
/// public address (nothing is sent).
fn lan_ip() -> IpAddr {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("192.0.2.1:80")?;
            s.local_addr()
        })
        .map_or(IpAddr::from([127, 0, 0, 1]), |a| a.ip())
}

// ─── HTTP ────────────────────────────────────────────────────────────────────

impl Server {
    fn start(store: Arc<MemoryBus>, port: u16) -> std::io::Result<Server> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::Builder::new()
            .name("floret-bus".into())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let store = store.clone();
                            std::thread::spawn(move || {
                                let _ = serve(stream, &store);
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(200)),
                    }
                }
            })?;
        Ok(Server {
            addr,
            lan: lan_ip(),
            stop,
        })
    }
}

struct Response {
    status: &'static str,
    content_type: Option<String>,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

fn serve(mut stream: TcpStream, store: &MemoryBus) -> std::io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        if buf.len() > MAX_HEADER {
            return Ok(());
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut req = lines.next().unwrap_or_default().split(' ');
    let (method, target) = (req.next().unwrap_or_default(), req.next().unwrap_or("/"));
    let header = |name: &str| {
        lines.clone().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case(name)
                .then(|| v.trim().to_owned())
        })
    };
    let length: usize = header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if length > MAX_BODY {
        return respond(&mut stream, status("413 Payload Too Large", ""));
    }
    let content_type = header("content-type");
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    let res = route(store, method, target, content_type, body);
    respond(&mut stream, res)
}

fn status(status: &'static str, text: &str) -> Response {
    Response {
        status,
        content_type: None,
        headers: Vec::new(),
        body: text.as_bytes().to_vec(),
    }
}

/// The relay's routes. Unlike the Vite middleware (which passes other
/// requests on to the dev server), anything else is a 404.
fn route(
    store: &MemoryBus,
    method: &str,
    target: &str,
    content_type: Option<String>,
    body: Vec<u8>,
) -> Response {
    let path = target.split('?').next().unwrap_or_default();
    let Some(rest) = path.strip_prefix(PREFIX) else {
        return status("404 Not Found", "");
    };
    let rest = rest.trim_start_matches('/');
    let (name, sub) = rest.split_once('/').unwrap_or((rest, ""));
    let name = percent_decode(name);
    if name.is_empty() {
        return status("404 Not Found", "");
    }
    let cors = ("Access-Control-Allow-Origin", "*".to_owned());
    match (method, sub) {
        ("POST", "") => {
            let ct = content_type.unwrap_or_else(|| "application/octet-stream".into());
            store.publish(&name, &ct, body);
            Response {
                headers: vec![cors],
                ..status("204 No Content", "")
            }
        }
        ("GET", "poll") => {
            let m = store.latest(&name);
            let json = serde_json::json!({
                "version": m.as_ref().map_or(0, |m| m.version),
                "has": m.is_some(),
                "contentType": m.as_ref().map(|m| m.content_type.clone()),
            });
            Response {
                status: "200 OK",
                content_type: Some("application/json".into()),
                headers: vec![cors],
                body: json.to_string().into_bytes(),
            }
        }
        ("GET", "") => match store.latest(&name) {
            None => status("404 Not Found", "no channel"),
            Some(BusMessage {
                version,
                content_type,
                body,
            }) => Response {
                status: "200 OK",
                content_type: Some(content_type),
                headers: vec![
                    cors,
                    ("Cache-Control", "no-store".into()),
                    ("X-Version", version.to_string()),
                ],
                body: body.to_vec(),
            },
        },
        _ => status("404 Not Found", ""),
    }
}

fn respond(stream: &mut TcpStream, r: Response) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        r.status,
        r.body.len()
    );
    if let Some(ct) = &r.content_type {
        head.push_str(&format!("Content-Type: {ct}\r\n"));
    }
    for (k, v) in &r.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(&r.body)?;
    stream.flush()
}

/// `%XX` escapes → bytes (what `encodeURIComponent` produces).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(addr: SocketAddr, raw: &[u8]) -> (String, Vec<u8>) {
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(raw).unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).unwrap();
        let split = out.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        (
            String::from_utf8_lossy(&out[..split]).into_owned(),
            out[split + 4..].to_vec(),
        )
    }

    #[test]
    fn relay_api_matches_the_dev_server() {
        let store = Arc::new(MemoryBus::new());
        let server = Server::start(store.clone(), 0).unwrap();
        let addr = SocketAddr::from(([127, 0, 0, 1], server.addr.port()));

        let (head, body) = request(addr, b"GET /__floret/bus/eink/poll HTTP/1.1\r\n\r\n");
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert_eq!(
            String::from_utf8(body).unwrap(),
            r#"{"version":0,"has":false,"contentType":null}"#
        );
        let (head, body) = request(addr, b"GET /__floret/bus/eink HTTP/1.1\r\n\r\n");
        assert!(head.starts_with("HTTP/1.1 404"));
        assert_eq!(body, b"no channel");

        let (head, _) = request(
            addr,
            b"POST /__floret/bus/my%20ch HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: 5\r\n\r\n[1,2]",
        );
        assert!(head.starts_with("HTTP/1.1 204"), "{head}");
        assert_eq!(&*store.latest("my ch").unwrap().body, b"[1,2]");

        store.publish("eink", "image/png", vec![1, 2, 3]);
        let (head, body) = request(addr, b"GET /__floret/bus/eink?x=1 HTTP/1.1\r\n\r\n");
        assert!(head.contains("Content-Type: image/png"), "{head}");
        assert!(head.contains("X-Version: 1"));
        assert_eq!(body, vec![1, 2, 3]);
        let (_, body) = request(addr, b"GET /__floret/bus/eink/poll HTTP/1.1\r\n\r\n");
        assert_eq!(
            String::from_utf8(body).unwrap(),
            r#"{"version":1,"has":true,"contentType":"image/png"}"#
        );
    }

    #[test]
    fn decodes_channel_names() {
        assert_eq!(percent_decode("a%2Fb%20c"), "a/b c");
        assert_eq!(percent_decode("bad%zz%"), "bad%zz%");
    }
}
