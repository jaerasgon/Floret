//! Floret desktop app.
//!
//! `floret [file.flora]` opens the app, with a patch file if one is given.
//! The window and everything in it is [`app::App`]; the built-in samples
//! are embedded at build time ([`SAMPLES`]).
//!
//! Environment: `FLORET_SAMPLE=<id>` starts on that sample (screenshots;
//! skips the welcome screen and recovery prompt). `FLORET_CONFIG_DIR` and
//! `FLORET_DATA_DIR` move preferences and autosaves / thumbnails.
//! `FLORET_EGUI_MENU=1` uses the in-window menu bar on macOS / Windows too.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod canvas;
mod crash;
mod devices;
mod editor;
mod export;
mod files;
mod fonts;
mod gpu_support;
mod help;
mod host;
mod library;
mod library_store;
mod menu;
mod node_help;
mod palette;
mod prefs;
mod presets_panel;
mod preview;
mod profiler;
mod recovery;
mod shader_panel;
mod theme;
mod thumbs;
mod welcome;
mod workspace;

use eframe::egui;

pub struct Sample {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub flora: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/samples.rs"));

fn main() -> eframe::Result {
    crash::install_hook();
    let launch = app::Launch {
        file: files::path_from_args(std::env::args_os()),
        sample: std::env::var("FLORET_SAMPLE").ok(),
    };
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: gpu_support::options(),
        run_and_return: true,
        viewport: egui::ViewportBuilder::default()
            .with_title("Floret")
            .with_app_id("floret")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Floret",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, launch)))),
    );
    if let Err(e) = &result {
        gpu_support::report_startup_failure(e);
    }
    result
}
