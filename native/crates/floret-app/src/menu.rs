//! Menus and app shortcuts (#73, #74).
//!
//! One command model ([`Command`], [`model`]) feeds two menu bars:
//! - **macOS / Windows**: a real OS menu bar via `muda` ([`native`]). On
//!   macOS it is the app's menu bar at the top of the screen; on Windows
//!   it sits in the window's frame.
//! - **Linux**: an egui menu bar in the toolbar ([`bar`]). `muda` can only
//!   attach to GTK windows there, and winit's windows aren't GTK.
//!
//! Shortcuts use Cmd on macOS and Ctrl elsewhere (egui's `COMMAND`).
//! Editing commands (undo, copy, …) are handled by the canvas's own key
//! handling; choosing them from a menu replays the key into egui
//! ([`event_for`])
//! so both paths do exactly the same thing. On macOS the menu bar receives
//! its key equivalents before the window does, which the replay also covers.

use crate::prefs::ThemePref;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    New,
    Open,
    OpenRecent(PathBuf),
    ClearRecent,
    Save,
    SaveAs,
    BrowseSamples,
    ExportPng,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Duplicate,
    Delete,
    SelectAll,
    Group,
    ToggleNodes,
    FitView,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Theme(ThemePref),
    Restart,
    Preferences,
    Shortcuts,
    Welcome,
    About,
}

const fn cmd(key: Key) -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, key)
}

const fn cmd_shift(key: Key) -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), key)
}

const fn plain(key: Key) -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::NONE, key)
}

impl Command {
    pub fn label(&self) -> &'static str {
        use Command::*;
        match self {
            New => "New",
            Open => "Open…",
            OpenRecent(_) => "Open Recent",
            ClearRecent => "Clear Recent",
            Save => "Save",
            SaveAs => "Save As…",
            BrowseSamples => "Browse Samples…",
            ExportPng => "Export PNG…",
            Quit => {
                if cfg!(target_os = "windows") {
                    "Exit"
                } else {
                    "Quit Floret"
                }
            }
            Undo => "Undo",
            Redo => "Redo",
            Cut => "Cut",
            Copy => "Copy",
            Paste => "Paste",
            Duplicate => "Duplicate",
            Delete => "Delete",
            SelectAll => "Select All",
            Group => "Group into Subpatch",
            ToggleNodes => "Nodes Panel",
            FitView => "Fit to View",
            ZoomIn => "Zoom In",
            ZoomOut => "Zoom Out",
            ZoomReset => "Actual Size",
            Theme(ThemePref::Dark) => "Dark",
            Theme(ThemePref::Light) => "Light",
            Theme(ThemePref::System) => "Use System Setting",
            Restart => "Restart Patch",
            Preferences => {
                if cfg!(target_os = "macos") {
                    "Settings…"
                } else {
                    "Preferences…"
                }
            }
            Shortcuts => "Keyboard Shortcuts",
            Welcome => "Welcome",
            About => "About Floret",
        }
    }

    /// The command's shortcut (shown in menus).
    pub fn shortcut(&self) -> Option<KeyboardShortcut> {
        use Command::*;
        Some(match self {
            New => cmd(Key::N),
            Open => cmd(Key::O),
            Save => cmd(Key::S),
            SaveAs => cmd_shift(Key::S),
            BrowseSamples => cmd_shift(Key::O),
            ExportPng => cmd(Key::E),
            Quit if cfg!(target_os = "windows") => return None,
            Quit => cmd(Key::Q),
            Undo => cmd(Key::Z),
            Redo if cfg!(target_os = "windows") => cmd(Key::Y),
            Redo => cmd_shift(Key::Z),
            Cut => cmd(Key::X),
            Copy => cmd(Key::C),
            Paste => cmd(Key::V),
            Duplicate => cmd(Key::D),
            Delete => plain(if cfg!(target_os = "macos") {
                Key::Backspace
            } else {
                Key::Delete
            }),
            SelectAll => cmd(Key::A),
            Group => cmd(Key::G),
            ToggleNodes => cmd(Key::Backslash),
            FitView => plain(Key::F),
            ZoomIn => cmd(Key::Equals),
            ZoomOut => cmd(Key::Minus),
            ZoomReset => cmd(Key::Num0),
            Restart => cmd(Key::R),
            Preferences => cmd(Key::Comma),
            Shortcuts => cmd(Key::Slash),
            _ => return None,
        })
    }

    /// Commands the canvas / preview already handle from the keyboard; a
    /// menu click replays the key ([`event_for`]).
    pub fn is_editing(&self) -> bool {
        use Command::*;
        matches!(
            self,
            Undo | Redo
                | Cut
                | Copy
                | Paste
                | Duplicate
                | Delete
                | SelectAll
                | Group
                | FitView
                | ExportPng
        )
    }
}

/// A menu entry.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Cmd(Command),
    /// A checkable command and whether it's checked.
    Check(Command, bool),
    Separator,
    /// The Open Recent submenu.
    Recent,
    /// The Theme submenu.
    Theme,
}

pub struct Section {
    pub title: &'static str,
    pub items: Vec<Item>,
}

/// State the menus reflect.
pub struct MenuState<'a> {
    pub recent: &'a [PathBuf],
    pub palette_visible: bool,
    pub theme: ThemePref,
}

/// The menu bar. On macOS, About / Settings / Quit live in the app menu
/// (added by [`native`]), so they're left out of File and Help.
pub fn model(s: &MenuState) -> Vec<Section> {
    use Command::*;
    use Item::{Cmd as C, Separator as Sep};
    let mac = cfg!(target_os = "macos");
    let mut file = vec![
        C(New),
        C(Open),
        Item::Recent,
        C(BrowseSamples),
        Sep,
        C(Save),
        C(SaveAs),
        Sep,
        C(ExportPng),
    ];
    if !mac {
        file.extend([Sep, C(Preferences), Sep, C(Quit)]);
    }
    let edit = vec![
        C(Undo),
        C(Redo),
        Sep,
        C(Cut),
        C(Copy),
        C(Paste),
        C(Duplicate),
        C(Delete),
        Sep,
        C(SelectAll),
        C(Group),
    ];
    let view = vec![
        Item::Check(ToggleNodes, s.palette_visible),
        Sep,
        C(FitView),
        Sep,
        C(ZoomIn),
        C(ZoomOut),
        C(ZoomReset),
        Item::Theme,
        Sep,
        C(Restart),
    ];
    let mut help = vec![C(Shortcuts), C(Welcome)];
    if !mac {
        help.extend([Sep, C(About)]);
    }
    vec![
        Section {
            title: "File",
            items: file,
        },
        Section {
            title: "Edit",
            items: edit,
        },
        Section {
            title: "View",
            items: view,
        },
        Section {
            title: "Help",
            items: help,
        },
    ]
}

/// App-level shortcuts typed in the window (not the editing ones, which
/// the canvas handles). On macOS the menu bar takes these first.
pub fn shortcuts(ctx: &egui::Context) -> Vec<Command> {
    use Command::*;
    // Most specific first: Cmd+Shift+S before Cmd+S.
    let order = [
        SaveAs,
        BrowseSamples,
        New,
        Open,
        Save,
        Quit,
        ToggleNodes,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        Restart,
        Preferences,
        Shortcuts,
    ];
    let mut out = Vec::new();
    ctx.input_mut(|i| {
        for c in order {
            let Some(sc) = c.shortcut() else { continue };
            if i.consume_shortcut(&sc) || (c == ZoomIn && i.consume_shortcut(&cmd(Key::Plus))) {
                out.push(c);
            }
        }
    });
    out
}

/// Modifiers as the platform reports Cmd / Ctrl.
fn platform(m: Modifiers) -> Modifiers {
    if m.command {
        Modifiers {
            mac_cmd: cfg!(target_os = "macos"),
            ctrl: !cfg!(target_os = "macos"),
            ..m
        }
    } else {
        m
    }
}

/// The input event an editing command replays, as if its key was typed
/// (the app feeds it to egui before the next frame).
pub fn event_for(c: &Command) -> Option<egui::Event> {
    Some(match c {
        Command::Copy => egui::Event::Copy,
        Command::Cut => egui::Event::Cut,
        Command::Paste => egui::Event::Paste(clipboard_text()?),
        // Redo replays Cmd+Shift+Z everywhere (the canvas accepts both).
        Command::Redo => key_event(cmd_shift(Key::Z)),
        other => key_event(other.shortcut()?),
    })
}

fn key_event(sc: KeyboardShortcut) -> egui::Event {
    egui::Event::Key {
        key: sc.logical_key,
        physical_key: Some(sc.logical_key),
        pressed: true,
        repeat: false,
        modifiers: platform(sc.modifiers),
    }
}

fn clipboard_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// "Ctrl+S", "Ctrl+=", "Cmd+Shift+Z": modifier names (egui's default font
/// has no ⌘ glyphs) and key symbols.
pub fn shortcut_text(sc: &KeyboardShortcut) -> String {
    let names = egui::ModifierNames {
        is_short: true,
        ..egui::ModifierNames::NAMES
    };
    sc.format(&names, cfg!(target_os = "macos"))
}

/// File name for a recent-files entry, with its folder for context.
pub fn recent_label(p: &std::path::Path) -> String {
    let name = p.file_name().map_or_else(
        || p.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    match p.parent().and_then(|d| d.file_name()) {
        Some(dir) => format!("{name} — {}", dir.to_string_lossy()),
        None => name,
    }
}

/// The egui menu bar (Linux, or wherever the native one is unavailable).
pub fn bar(ui: &mut egui::Ui, state: &MenuState) -> Option<Command> {
    let mut picked = None;
    egui::MenuBar::new().ui(ui, |ui| {
        for section in model(state) {
            ui.menu_button(section.title, |ui| {
                ui.set_min_width(220.0);
                for item in &section.items {
                    if let Some(c) = egui_item(ui, item, state) {
                        picked = Some(c);
                        ui.close();
                    }
                }
            });
        }
    });
    picked
}

fn egui_item(ui: &mut egui::Ui, item: &Item, state: &MenuState) -> Option<Command> {
    let button = |ui: &mut egui::Ui, c: &Command, checked: Option<bool>| {
        let mut text = c.label().to_owned();
        if let Some(on) = checked {
            text = format!("{} {text}", if on { "✔" } else { "  " });
        }
        let mut b = egui::Button::new(text);
        if let Some(sc) = c.shortcut() {
            b = b.shortcut_text(shortcut_text(&sc));
        }
        ui.add(b).clicked().then(|| c.clone())
    };
    match item {
        Item::Cmd(c) => button(ui, c, None),
        Item::Check(c, on) => button(ui, c, Some(*on)),
        Item::Separator => {
            ui.separator();
            None
        }
        Item::Recent => {
            let mut picked = None;
            ui.add_enabled_ui(!state.recent.is_empty(), |ui| {
                ui.menu_button("Open Recent", |ui| {
                    for p in state.recent {
                        if ui
                            .button(recent_label(p))
                            .on_hover_text(p.display().to_string())
                            .clicked()
                        {
                            picked = Some(Command::OpenRecent(p.clone()));
                        }
                    }
                    ui.separator();
                    if ui.button(Command::ClearRecent.label()).clicked() {
                        picked = Some(Command::ClearRecent);
                    }
                });
            });
            picked
        }
        Item::Theme => {
            let mut picked = None;
            ui.menu_button("Theme", |ui| {
                for t in [ThemePref::Dark, ThemePref::Light, ThemePref::System] {
                    let c = Command::Theme(t);
                    if let Some(c) = button(ui, &c, Some(state.theme == t)) {
                        picked = Some(c);
                    }
                }
            });
            picked
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod native {
    //! The OS menu bar via `muda`, built from [`super::model`].

    use super::{model, recent_label, Command, Item, MenuState};
    use crate::prefs::ThemePref;
    use eframe::egui::{Key, KeyboardShortcut};
    use muda::accelerator::{Accelerator, Code, Modifiers as M};
    use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    pub struct NativeMenu {
        menu: Menu,
        /// Item id → command.
        ids: Vec<(muda::MenuId, Command)>,
        /// What the menu was last built from.
        built: (Vec<std::path::PathBuf>, bool, ThemePref),
        #[cfg(target_os = "windows")]
        hwnd: isize,
    }

    impl NativeMenu {
        /// Build the menu bar and attach it to the app (macOS) or the
        /// window (Windows). `None` if that isn't possible.
        pub fn new(
            window: &impl raw_window_handle::HasWindowHandle,
            state: &MenuState,
        ) -> Option<NativeMenu> {
            #[cfg(target_os = "windows")]
            let hwnd = match window.window_handle().ok()?.as_raw() {
                raw_window_handle::RawWindowHandle::Win32(h) => h.hwnd.get(),
                _ => return None,
            };
            #[cfg(target_os = "macos")]
            let _ = window;
            let mut m = NativeMenu {
                menu: Menu::new(),
                ids: Vec::new(),
                built: (Vec::new(), false, ThemePref::Dark),
                #[cfg(target_os = "windows")]
                hwnd,
            };
            m.build(state).ok()?;
            #[cfg(target_os = "macos")]
            m.menu.init_for_nsapp();
            #[cfg(target_os = "windows")]
            {
                let theme = if crate::theme::is_light() {
                    muda::MenuTheme::Light
                } else {
                    muda::MenuTheme::Dark
                };
                // SAFETY: `hwnd` is eframe's live main window, and this
                // runs on the thread that owns it.
                unsafe { m.menu.init_for_hwnd_with_theme(hwnd, theme) }.ok()?;
            }
            Some(m)
        }

        fn build(&mut self, s: &MenuState) -> muda::Result<()> {
            for item in self.menu.items() {
                if let muda::MenuItemKind::Submenu(sub) = item {
                    self.menu.remove(&sub)?;
                }
            }
            self.ids.clear();
            #[cfg(target_os = "macos")]
            {
                let app = Submenu::new("Floret", true);
                app.append(&PredefinedMenuItem::about(None, None))?;
                app.append(&PredefinedMenuItem::separator())?;
                let prefs = self.item(&Command::Preferences);
                app.append(&prefs)?;
                app.append(&PredefinedMenuItem::separator())?;
                app.append(&PredefinedMenuItem::services(None))?;
                app.append(&PredefinedMenuItem::separator())?;
                app.append(&PredefinedMenuItem::hide(None))?;
                app.append(&PredefinedMenuItem::hide_others(None))?;
                app.append(&PredefinedMenuItem::show_all(None))?;
                app.append(&PredefinedMenuItem::separator())?;
                // Our own Quit, so unsaved changes are asked about.
                let quit = self.item(&Command::Quit);
                app.append(&quit)?;
                self.menu.append(&app)?;
            }
            for section in model(s) {
                let sub = Submenu::new(native_title(section.title), true);
                for item in &section.items {
                    match item {
                        Item::Cmd(c) => sub.append(&self.item(c))?,
                        Item::Check(c, on) => {
                            let id = self.id(c);
                            sub.append(&CheckMenuItem::with_id(
                                id,
                                c.label(),
                                true,
                                *on,
                                accel(c),
                            ))?;
                        }
                        Item::Separator => sub.append(&PredefinedMenuItem::separator())?,
                        Item::Recent => {
                            let recent = Submenu::new("Open Recent", !s.recent.is_empty());
                            for p in s.recent {
                                let c = Command::OpenRecent(p.clone());
                                let id = self.id(&c);
                                recent.append(&MenuItem::with_id(
                                    id,
                                    recent_label(p),
                                    true,
                                    None,
                                ))?;
                            }
                            recent.append(&PredefinedMenuItem::separator())?;
                            recent.append(&self.item(&Command::ClearRecent))?;
                            sub.append(&recent)?;
                        }
                        Item::Theme => {
                            let theme = Submenu::new("Theme", true);
                            for t in [ThemePref::Dark, ThemePref::Light, ThemePref::System] {
                                let c = Command::Theme(t);
                                let id = self.id(&c);
                                theme.append(&CheckMenuItem::with_id(
                                    id,
                                    c.label(),
                                    true,
                                    s.theme == t,
                                    None,
                                ))?;
                            }
                            sub.append(&theme)?;
                        }
                    }
                }
                #[cfg(target_os = "macos")]
                if section.title == "Help" {
                    sub.set_as_help_menu_for_nsapp();
                }
                self.menu.append(&sub)?;
                #[cfg(target_os = "macos")]
                if section.title == "View" {
                    let window = Submenu::new("Window", true);
                    window.append(&PredefinedMenuItem::minimize(None))?;
                    window.append(&PredefinedMenuItem::maximize(None))?;
                    window.append(&PredefinedMenuItem::fullscreen(None))?;
                    window.set_as_windows_menu_for_nsapp();
                    self.menu.append(&window)?;
                }
            }
            self.built = (s.recent.to_vec(), s.palette_visible, s.theme);
            Ok(())
        }

        fn id(&mut self, c: &Command) -> muda::MenuId {
            let id = muda::MenuId::new(format!("floret-{}", self.ids.len()));
            self.ids.push((id.clone(), c.clone()));
            id
        }

        fn item(&mut self, c: &Command) -> MenuItem {
            let id = self.id(c);
            MenuItem::with_id(id, c.label(), true, accel(c))
        }

        /// Rebuild when what the menu shows changed (recent files, checks).
        pub fn sync(&mut self, s: &MenuState) {
            if self.built != (s.recent.to_vec(), s.palette_visible, s.theme) {
                let _ = self.build(s);
            }
            #[cfg(target_os = "windows")]
            {
                let theme = if crate::theme::is_light() {
                    muda::MenuTheme::Light
                } else {
                    muda::MenuTheme::Dark
                };
                // SAFETY: as in `new`.
                let _ = unsafe { self.menu.set_theme_for_hwnd(self.hwnd, theme) };
            }
        }

        /// Commands chosen since the last call.
        pub fn poll(&self) -> Vec<Command> {
            let mut out = Vec::new();
            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                if let Some((_, c)) = self.ids.iter().find(|(id, _)| *id == ev.id) {
                    out.push(c.clone());
                }
            }
            out
        }
    }

    /// Windows menus underline an access key after `&`.
    fn native_title(t: &str) -> String {
        if cfg!(target_os = "windows") {
            format!("&{t}")
        } else {
            t.to_owned()
        }
    }

    /// The menu accelerator for a command. Unmodified keys (F, Delete) get
    /// none on macOS, where the menu would take that key from text fields;
    /// on Windows accelerators only label the item (keys reach egui).
    fn accel(c: &Command) -> Option<Accelerator> {
        let sc: KeyboardShortcut = c.shortcut()?;
        let m = sc.modifiers;
        if cfg!(target_os = "macos") && !m.command {
            return None;
        }
        let mut mods = M::empty();
        if m.command {
            mods |= muda::accelerator::CMD_OR_CTRL;
        }
        if m.shift {
            mods |= M::SHIFT;
        }
        if m.alt {
            mods |= M::ALT;
        }
        let code = match sc.logical_key {
            Key::A => Code::KeyA,
            Key::C => Code::KeyC,
            Key::D => Code::KeyD,
            Key::E => Code::KeyE,
            Key::F => Code::KeyF,
            Key::G => Code::KeyG,
            Key::N => Code::KeyN,
            Key::O => Code::KeyO,
            Key::Q => Code::KeyQ,
            Key::R => Code::KeyR,
            Key::S => Code::KeyS,
            Key::V => Code::KeyV,
            Key::X => Code::KeyX,
            Key::Y => Code::KeyY,
            Key::Z => Code::KeyZ,
            Key::Num0 => Code::Digit0,
            Key::Equals => Code::Equal,
            Key::Minus => Code::Minus,
            Key::Comma => Code::Comma,
            Key::Slash => Code::Slash,
            Key::Backslash => Code::Backslash,
            Key::Delete => Code::Delete,
            Key::Backspace => Code::Backspace,
            _ => return None,
        };
        Some(Accelerator::new(mods, code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One frame of a headless egui.
    fn frame(ctx: &egui::Context, raw: egui::RawInput, f: impl FnMut(&mut egui::Ui)) {
        ctx.run_ui(raw, f).textures_delta.clear();
    }

    #[test]
    fn every_menu_shortcut_is_unique() {
        let state = MenuState {
            recent: &[],
            palette_visible: true,
            theme: ThemePref::Dark,
        };
        let mut seen: Vec<(KeyboardShortcut, &'static str)> = Vec::new();
        for s in model(&state) {
            for item in s.items {
                if let Item::Cmd(c) | Item::Check(c, _) = item {
                    if let Some(sc) = c.shortcut() {
                        assert!(
                            !seen.iter().any(|(x, _)| *x == sc),
                            "{} reuses a shortcut",
                            c.label()
                        );
                        seen.push((sc, c.label()));
                    }
                }
            }
        }
        assert!(seen.len() > 15);
    }

    #[test]
    fn shortcuts_consume_save_as_before_save() {
        let ctx = egui::Context::default();
        let press = |m: Modifiers, k: Key| egui::RawInput {
            events: vec![egui::Event::Key {
                key: k,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: platform(m),
            }],
            ..Default::default()
        };
        let mut got = Vec::new();
        frame(
            &ctx,
            press(Modifiers::COMMAND | Modifiers::SHIFT, Key::S),
            |ui| {
                got = shortcuts(ui.ctx());
            },
        );
        assert_eq!(got, [Command::SaveAs]);
        frame(&ctx, press(Modifiers::COMMAND, Key::S), |ui| {
            got = shortcuts(ui.ctx());
        });
        assert_eq!(got, [Command::Save]);
        // Plain keys are the canvas's.
        frame(&ctx, press(Modifiers::NONE, Key::S), |ui| {
            got = shortcuts(ui.ctx());
        });
        assert!(got.is_empty());
    }

    #[test]
    fn replayed_keys_match_what_the_canvas_consumes() {
        let ctx = egui::Context::default();
        let mut undo = false;
        let mut redo = false;
        let raw = egui::RawInput {
            events: [Command::Redo, Command::Undo]
                .iter()
                .filter_map(event_for)
                .collect(),
            ..Default::default()
        };
        frame(&ctx, raw, |ui| {
            ui.input_mut(|i| {
                let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
                redo = i.consume_key(cmd_shift, Key::Z);
                undo = i.consume_key(Modifiers::COMMAND, Key::Z);
            });
        });
        assert!(undo && redo);
    }
}
