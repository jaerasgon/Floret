//! GPU setup and the "GPU not supported" message (#76).
//!
//! Adapter choice: every backend wgpu was built with (Metal / D3D12 /
//! Vulkan, then GL), best first — a hardware GPU on a native backend, then
//! one on GL, then a software adapter. The device asks only for limits the
//! adapter has. If nothing works the app shows a native message dialog
//! with an adapter report instead of exiting silently.

use eframe::egui_wgpu::{self, wgpu, WgpuSetup, WgpuSetupCreateNew};
use std::sync::Arc;

pub fn options() -> egui_wgpu::WgpuConfiguration {
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.native_adapter_selector = Some(Arc::new(|adapters, surface| {
        adapters
            .iter()
            .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
            .min_by_key(|a| {
                let i = a.get_info();
                rank(i.device_type, i.backend)
            })
            .cloned()
            .ok_or_else(|| {
                if adapters.is_empty() {
                    "no graphics adapter was found".to_owned()
                } else {
                    "no graphics adapter can draw to this window".to_owned()
                }
            })
    }));
    setup.device_descriptor = Arc::new(|adapter| {
        let has = adapter.limits();
        let limits = [
            wgpu::Limits::default(),
            wgpu::Limits::downlevel_defaults(),
            wgpu::Limits::downlevel_webgl2_defaults(),
        ]
        .into_iter()
        // Big enough for 4K windows and 4× exports, if the GPU allows.
        .map(|l| wgpu::Limits {
            max_texture_dimension_2d: has.max_texture_dimension_2d.min(16384),
            ..l
        })
        .find(|l| l.check_limits(&has))
        .unwrap_or(has);
        wgpu::DeviceDescriptor {
            label: Some("floret device"),
            required_limits: limits,
            ..Default::default()
        }
    });
    egui_wgpu::WgpuConfiguration {
        wgpu_setup: WgpuSetup::CreateNew(setup),
        ..Default::default()
    }
}

/// Lower is better: hardware before software, native backends before GL,
/// discrete before integrated (eframe's default power preference).
fn rank(device_type: wgpu::DeviceType, backend: wgpu::Backend) -> (u8, u8) {
    let device = match device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Other => 3,
        wgpu::DeviceType::Cpu => 4,
    };
    let software = u8::from(device == 4);
    let gl = u8::from(backend == wgpu::Backend::Gl);
    (software * 2 + gl, device)
}

/// Every adapter wgpu can see, one per line (for the error report).
pub fn adapter_report() -> String {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
    if adapters.is_empty() {
        return "No graphics adapters found.".to_owned();
    }
    adapters
        .iter()
        .map(|a| {
            let i = a.get_info();
            format!(
                "• {} ({:?}, {:?}) driver {} {}",
                i.name, i.backend, i.device_type, i.driver, i.driver_info
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Tell the user why the app couldn't start: a native dialog (also printed
/// to stderr, for terminals and logs).
pub fn report_startup_failure(err: &eframe::Error) {
    let gpu = matches!(err, eframe::Error::Wgpu(_));
    let title = if gpu {
        "Floret can't use your graphics card"
    } else {
        "Floret couldn't start"
    };
    let advice = if gpu {
        "Floret draws everything on the GPU and needs Metal (macOS), Direct3D 12 \
         (Windows), Vulkan or OpenGL 3.3 / OpenGL ES 3.0 (Linux).\n\n\
         Try updating your graphics drivers. On Linux, make sure a Vulkan driver \
         (e.g. Mesa) is installed; on virtual machines, enable 3D acceleration."
    } else {
        "The app window couldn't be created."
    };
    let text = format!(
        "{advice}\n\nError: {err}\n\nGraphics adapters:\n{}",
        adapter_report()
    );
    eprintln!("{title}\n\n{text}");
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_native_backends_rank_first() {
        use wgpu::{Backend::*, DeviceType::*};
        let mut v = vec![
            (Cpu, Vulkan),
            (IntegratedGpu, Gl),
            (IntegratedGpu, Vulkan),
            (DiscreteGpu, Vulkan),
        ];
        v.sort_by_key(|&(d, b)| rank(d, b));
        assert_eq!(
            v,
            [
                (DiscreteGpu, Vulkan),
                (IntegratedGpu, Vulkan),
                (IntegratedGpu, Gl),
                (Cpu, Vulkan)
            ]
        );
    }
}
