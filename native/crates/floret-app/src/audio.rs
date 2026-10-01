//! Sound: the app's one audio engine (`floret-media`), lent to every
//! engine the app builds, plus the status-bar control for volume, mute,
//! devices and the microphone.
//!
//! The device opens when the app starts (or the engine runs silently on a
//! timer if there is none). The microphone opens only while a patch's
//! `sound/audio-in` is started, so the OS permission prompt appears then;
//! a refusal shows here and the node just outputs nothing.

use crate::theme;
use eframe::egui::{self, RichText};
use floret_engine::audio::MicState;
use floret_engine::Engine;
use floret_media::{AudioSystem, DeviceInfo};
use std::sync::{Arc, OnceLock};

pub fn shared() -> Arc<AudioSystem> {
    static AUDIO: OnceLock<Arc<AudioSystem>> = OnceLock::new();
    AUDIO
        .get_or_init(|| {
            let a = AudioSystem::start();
            a.set_host(Some(crate::host::shared()));
            Arc::new(a)
        })
        .clone()
}

/// Connect a freshly compiled engine's sound nodes to the speakers.
pub fn attach(engine: &mut Engine) {
    engine.set_audio(Some(shared()));
}

/// Silence everything (another document is being opened).
pub fn clear() {
    shared().clear();
}

fn device_picker(
    ui: &mut egui::Ui,
    salt: &str,
    devices: &[DeviceInfo],
    selected: &Option<String>,
) -> Option<Option<String>> {
    let name_of = |id: &Option<String>| match id {
        None => "System default".to_owned(),
        Some(id) => devices
            .iter()
            .find(|d| &d.id == id)
            .map_or_else(|| id.clone(), |d| d.name.clone()),
    };
    let mut pick = None;
    egui::ComboBox::from_id_salt(salt)
        .width(220.0)
        .selected_text(name_of(selected))
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(selected.is_none(), "System default")
                .clicked()
            {
                pick = Some(None);
            }
            for d in devices {
                let on = selected.as_deref() == Some(d.id.as_str());
                if ui.selectable_label(on, &d.name).clicked() {
                    pick = Some(Some(d.id.clone()));
                }
            }
        });
    pick
}

/// The status-bar control: a speaker button with a settings popup.
pub fn status(ui: &mut egui::Ui) {
    let audio = shared();
    let st = audio.status();
    let master = audio.master();
    let icon = if master.muted() || master.volume() == 0.0 {
        "🔇"
    } else {
        "🔊"
    };
    let mic = match &st.mic {
        MicState::Live => " · 🎤",
        _ => "",
    };
    let color = if st.output.is_some() {
        theme::FG_1
    } else {
        theme::FG_2
    };
    let tip = match (&st.output, &st.error) {
        (Some(name), _) => format!("Sound: {name}"),
        (None, Some(e)) => format!("No sound output: {e}"),
        (None, None) => "No sound output".to_owned(),
    };
    ui.menu_button(
        RichText::new(format!("{icon}{mic}")).color(color).small(),
        |ui| {
            ui.set_min_width(260.0);
            ui.set_max_width(300.0);
            ui.label(RichText::new("Sound").strong());
            let mut muted = master.muted();
            let mut volume = master.volume();
            ui.horizontal(|ui| {
                if ui.checkbox(&mut muted, "Mute").changed() {
                    master.set_muted(muted);
                }
                if ui
                    .add(egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false))
                    .changed()
                {
                    master.set_volume(volume);
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Output").color(theme::FG_1).small());
            if let Some(id) = device_picker(ui, "audio-out", &st.outputs, &st.selected_output) {
                audio.set_output_device(id);
            }
            match (&st.output, &st.error) {
                (Some(_), _) => {
                    ui.label(
                        RichText::new(format!("{} Hz", st.sample_rate))
                            .color(theme::FG_2)
                            .small(),
                    );
                }
                (None, e) => {
                    let why = e.as_deref().unwrap_or("no device");
                    ui.label(
                        RichText::new(format!("Running silently: {why}"))
                            .color(theme::DANGER)
                            .small(),
                    );
                }
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Microphone").color(theme::FG_1).small());
            if let Some(id) = device_picker(ui, "audio-in", &st.inputs, &st.selected_input) {
                audio.set_input_device(id);
            }
            let (text, c) = match &st.mic {
                MicState::Off => (
                    "Off — opens when an AudioIn node starts".to_owned(),
                    theme::FG_2,
                ),
                MicState::Starting => ("Starting… (allow access if asked)".to_owned(), theme::FG_1),
                MicState::Live => ("Listening".to_owned(), theme::GOLD),
                MicState::Failed(e) => (
                    format!("Unavailable: {e}. Check the OS privacy settings."),
                    theme::DANGER,
                ),
            };
            ui.label(RichText::new(text).color(c).small());
            ui.add_space(4.0);
            if ui.small_button("Refresh devices").clicked() {
                audio.refresh_devices();
            }
        },
    )
    .response
    .on_hover_text(tip);
}
