//! What the app lends the engine (`floret_engine::host`): GPU rasterising
//! for `image/from-layer` / `transform/cache`, and HTTP for
//! `io/image-url`. One shared instance; its GPU device is only created
//! when a patch first rasterises.

use floret_engine::host::Host;
use floret_engine::image::Image;
use floret_engine::scene::SceneNode;
use floret_render::LayerRasterizer;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// Largest download accepted for an image URL.
const MAX_FETCH_BYTES: u64 = 64 * 1024 * 1024;

struct AppHost {
    raster: LayerRasterizer,
    http: ureq::Agent,
}

impl Host for AppHost {
    fn rasterize(&self, layers: &[Arc<SceneNode>], width: u32, height: u32) -> Option<Image> {
        self.raster.rasterize(layers, width, height)
    }

    fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        let mut res = self
            .http
            .get(url)
            .call()
            .map_err(|e| format!("{url}: {e}"))?;
        res.body_mut()
            .with_config()
            .limit(MAX_FETCH_BYTES)
            .read_to_vec()
            .map_err(|e| format!("{url}: {e}"))
    }
}

pub fn shared() -> Arc<dyn Host> {
    static HOST: OnceLock<Arc<AppHost>> = OnceLock::new();
    HOST.get_or_init(|| {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        Arc::new(AppHost {
            raster: LayerRasterizer::new(),
            http: ureq::Agent::new_with_config(config),
        })
    })
    .clone()
}
