//! What the app lends the engine (`floret_engine::host`): GPU rasterising
//! for `image/from-layer` / `transform/cache`, HTTP for `io/image-url`,
//! and the open patch's folder for relative paths. One shared instance;
//! its GPU device is only created when a patch first rasterises.

use floret_engine::host::Host;
use floret_engine::image::Image;
use floret_engine::scene::SceneNode;
use floret_render::LayerRasterizer;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

/// Largest download accepted for an image URL.
const MAX_FETCH_BYTES: u64 = 64 * 1024 * 1024;

struct AppHost {
    raster: LayerRasterizer,
    http: ureq::Agent,
    base_dir: RwLock<Option<PathBuf>>,
}

impl Host for AppHost {
    fn rasterize(&self, layers: &[Arc<SceneNode>], width: u32, height: u32) -> Option<Image> {
        self.raster.rasterize(layers, width, height)
    }

    fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        fetch(&self.http, url)
    }

    fn base_dir(&self) -> Option<PathBuf> {
        self.base_dir.read().ok()?.clone()
    }
}

fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(60)))
        .max_redirects(10)
        .user_agent(concat!("Floret/", env!("CARGO_PKG_VERSION")))
        .accept("image/*, */*;q=0.8")
        // Statuses are reported below, with the server's reason.
        .http_status_as_error(false)
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build();
    ureq::Agent::new_with_config(config)
}

/// GET `url`, following redirects; a readable error otherwise.
fn fetch(http: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    let mut res = http.get(url).call().map_err(|e| describe(&e))?;
    let status = res.status();
    if !status.is_success() {
        let reason = status.canonical_reason().unwrap_or("");
        return Err(format!("the server answered {} {reason}", status.as_u16()));
    }
    res.body_mut()
        .with_config()
        .limit(MAX_FETCH_BYTES)
        .read_to_vec()
        .map_err(|e| describe(&e))
}

fn describe(e: &ureq::Error) -> String {
    use ureq::Error as E;
    match e {
        E::Timeout(_) => "the server took too long to answer".into(),
        E::HostNotFound => {
            "couldn't find that server (check the address and your connection)".into()
        }
        E::ConnectionFailed => "couldn't connect to the server".into(),
        E::TooManyRedirects => "too many redirects".into(),
        E::RedirectFailed => "a redirect went nowhere".into(),
        E::BadUri(_) => "that isn't a valid URL".into(),
        E::BodyExceedsLimit(_) => "the file is too large (over 64 MB)".into(),
        E::ConnectProxyFailed(m) => format!("the proxy refused the connection ({m})"),
        E::Io(io) => format!("network error: {io}"),
        other => other.to_string(),
    }
}

fn shared_host() -> &'static Arc<AppHost> {
    static HOST: OnceLock<Arc<AppHost>> = OnceLock::new();
    HOST.get_or_init(|| {
        Arc::new(AppHost {
            raster: LayerRasterizer::new(),
            http: agent(),
            base_dir: RwLock::new(None),
        })
    })
}

pub fn shared() -> Arc<dyn Host> {
    shared_host().clone()
}

/// The open patch's file (`None` while untitled): relative paths in it
/// resolve against its folder.
pub fn set_document(path: Option<&Path>) {
    let dir = path
        .and_then(Path::parent)
        .map(|d| d.canonicalize().unwrap_or_else(|_| d.to_owned()));
    if let Ok(mut b) = shared_host().base_dir.write() {
        *b = dir;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs the network: `cargo test -p floret-app -- --ignored fetch`.
    /// (The node's default, picsum.photos, answers with a redirect.)
    #[test]
    #[ignore]
    fn fetches_over_https() {
        let http = agent();
        let raw = "https://raw.githubusercontent.com/github/explore/main/topics/rust";
        let bytes = fetch(&http, &format!("{raw}/rust.png")).unwrap();
        let img = floret_engine::image::decode(&bytes).unwrap();
        assert!(img.width() > 1 && img.height() > 1);
        let e = fetch(&http, &format!("{raw}/missing.png")).unwrap_err();
        assert!(e.contains("404"), "{e}");
        let e = fetch(&http, "https://no-such-host.invalid/x.png").unwrap_err();
        assert!(!e.is_empty());
    }
}
