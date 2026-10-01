//! Offscreen rendering on a device of our own: tests, thumbnails, export.

use crate::gpu::{RenderStats, Renderer, STENCIL_FORMAT};
use floret_engine::scene::SceneNode;
use std::sync::Arc;

/// The offscreen / readback colour format.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A device + renderer for offscreen frames. Reuse one for many renders:
/// creating a device is the slow part.
pub struct Headless {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub renderer: Renderer,
    pub adapter_info: wgpu::AdapterInfo,
}

impl Headless {
    /// `None` when no adapter / device is available (e.g. CI without a GPU
    /// or a software Vulkan). Honours `WGPU_BACKEND` & co. Uses 4× MSAA when
    /// the adapter supports it for our formats.
    pub fn new() -> Option<Headless> {
        Self::with_sample_count(4)
    }

    /// Like [`Headless::new`] with a given MSAA level (falls back to 1 when
    /// the adapter can't multisample our formats at that count).
    pub fn with_sample_count(samples: u32) -> Option<Headless> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .ok()?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("floret-render headless"),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .ok()?;
        let msaa = [FORMAT, STENCIL_FORMAT].iter().all(|f| {
            adapter
                .get_texture_format_features(*f)
                .flags
                .sample_count_supported(samples)
        });
        let samples = if msaa { samples } else { 1 };
        let renderer = Renderer::new(&device, &queue, FORMAT, samples);
        Some(Headless {
            device,
            queue,
            renderer,
            adapter_info: adapter.get_info(),
        })
    }

    /// Render one frame to tightly packed RGBA8 (premultiplied; opaque
    /// wherever the background shows).
    pub fn render(
        &mut self,
        layers: &[Arc<SceneNode>],
        width: u32,
        height: u32,
        background: [f64; 4],
    ) -> Vec<u8> {
        self.render_with_stats(layers, width, height, background).0
    }

    pub fn render_with_stats(
        &mut self,
        layers: &[Arc<SceneNode>],
        width: u32,
        height: u32,
        background: [f64; 4],
    ) -> (Vec<u8>, RenderStats) {
        render_and_read(
            &self.device,
            &self.queue,
            &mut self.renderer,
            layers,
            (width, height),
            background,
        )
    }
}

/// Render one frame offscreen with any device + [`Renderer`] and read it
/// back as tightly packed RGBA8 (premultiplied; opaque wherever the
/// background shows). The renderer must have been created with an
/// `Rgba8Unorm` format. Blocks until the GPU is done — meant for export
/// (PNG, #54) and tests, not the live preview. The app can call this with
/// its own (egui) device and a renderer kept for exports.
pub fn render_and_read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    layers: &[Arc<SceneNode>],
    size: (u32, u32),
    background: [f64; 4],
) -> (Vec<u8>, RenderStats) {
    debug_assert_eq!(renderer.format(), FORMAT);
    let (w, h) = (size.0.max(1), size.1.max(1));
    let size = wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("floret offscreen target"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let row = w * 4;
    let padded = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("floret offscreen readback"),
        size: u64::from(padded) * u64::from(h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("floret offscreen"),
    });
    let stats = renderer.render(
        device,
        queue,
        &mut encoder,
        &view,
        layers,
        background,
        (w, h),
    );
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let Ok(data) = slice.get_mapped_range() else {
        return (vec![0; (row * h) as usize], stats);
    };
    let mut out = Vec::with_capacity((row * h) as usize);
    for y in 0..h as usize {
        let start = y * padded as usize;
        out.extend_from_slice(&data[start..start + row as usize]);
    }
    drop(data);
    readback.unmap();
    (out, stats)
}

/// One-shot offscreen render: RGBA8, `width × height`. `None` when no GPU
/// adapter is available (tests skip instead of failing).
pub fn render_to_rgba(
    layers: &[Arc<SceneNode>],
    width: u32,
    height: u32,
    background: [f64; 4],
) -> Option<Vec<u8>> {
    Some(Headless::new()?.render(layers, width, height, background))
}
