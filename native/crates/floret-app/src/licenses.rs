//! Help → Licenses…: Floret's own licence and the notices for everything
//! it ships with (fonts, icons, tracking models, Rust crates).
//!
//! Every text is embedded with `include_str!`, so nothing is read from disk
//! at run time. The window has a list of [`Section`]s on the left and a
//! scrollable text pane on the right. The pane shows a section as
//! pre-wrapped lines and lays out only the lines in view
//! ([`egui::ScrollArea::show_rows`]), so the long Rust crates section (a
//! table of every dependency and the full licence texts) opens at once.
//!
//! The crate list and licence texts under `native/licenses/` are generated
//! by `tools/gen-licenses.py` from `cargo metadata`; the tests at the
//! bottom fail when they fall behind `Cargo.lock` or when a dependency is
//! under a licence outside the allow-list.

use crate::theme::{BG_1, BORDER, FG_0, FG_1, FG_2, GOLD_TEXT};
use eframe::egui::{self, RichText};

/// Floret's own licence.
const FLORET_LICENSE: &str = include_str!("../../../../LICENSE");
/// `THIRD_PARTY_NOTICES.md`.
const NOTICES: &str = include_str!("../../../../THIRD_PARTY_NOTICES.md");

const INTER_LICENSE: &str = include_str!("../assets/fonts/LICENSE-Inter.txt");
const DEJAVU_LICENSE: &str = include_str!("../../floret-engine/fonts/LICENSE-DejaVu.txt");
const LIBERATION_LICENSE: &str = include_str!("../../floret-engine/fonts/LICENSE-Liberation.txt");
const ROBOTO_FLEX_LICENSE: &str = include_str!("../../floret-engine/fonts/LICENSE-RobotoFlex.txt");
// egui's built-in fonts (the `epaint_default_fonts` crate).
const HACK_LICENSE: &str = include_str!("../assets/licenses/Hack.txt");
const NOTO_EMOJI_LICENSE: &str = include_str!("../assets/licenses/Noto-Emoji-OFL.txt");
const EMOJI_ICON_FONT_LICENSE: &str = include_str!("../assets/licenses/emoji-icon-font-MIT.txt");
const UBUNTU_LICENSE: &str = include_str!("../assets/licenses/Ubuntu-Font-Licence.txt");

const PHOSPHOR_LICENSE: &str = include_str!("../assets/licenses/Phosphor-MIT.txt");
const EGUI_PHOSPHOR_LICENSE: &str = include_str!("../assets/licenses/egui-phosphor-MIT.txt");

const MEDIAPIPE_NOTICE: &str = include_str!("../../../../public/mediapipe/NOTICE");
const MEDIAPIPE_LICENSE: &str = include_str!("../../../../public/mediapipe/LICENSE");

/// The generated crate table (`native/licenses/crates.md`) and the licence
/// texts and notices that go with it.
const CRATES_TABLE: &str = include_str!("../../../licenses/crates.md");
const CRATE_LICENSE_TEXTS: &str = include_str!("../../../licenses/licence-texts.txt");

/// Where the text is wrapped, in monospace columns.
const WRAP_COLUMNS: usize = 88;

/// One entry of the section list.
pub struct Section {
    pub title: &'static str,
    /// What the section covers, shown under its title.
    pub blurb: &'static str,
    pub text: String,
}

/// All sections, in list order.
pub fn sections() -> Vec<Section> {
    vec![
        Section {
            title: "Floret",
            blurb: "Floret's own licence.",
            text: plain(FLORET_LICENSE),
        },
        Section {
            title: "Fonts",
            blurb: "The interface font, the fonts the Text nodes can draw with, and the fonts built into the interface toolkit.",
            text: fonts_text(),
        },
        Section {
            title: "Icons",
            blurb: "Interface icons.",
            text: icons_text(),
        },
        Section {
            title: "Tracking models",
            blurb: "The hand, face and pose models behind the Landmarks node.",
            text: tracking_text(),
        },
        Section {
            title: "Rust crates",
            blurb: "Every open-source library compiled into Floret: name, version, licence, and the licence texts.",
            text: crates_text(),
        },
        Section {
            title: "Notices",
            blurb: "THIRD_PARTY_NOTICES.md: what is bundled, from where, under which terms.",
            text: plain(NOTICES),
        },
    ]
}

/// A heading, an optional note, then each licence text under a rule.
fn block(out: &mut String, title: &str, note: &str, texts: &[&str]) {
    out.push_str(title);
    out.push('\n');
    out.push_str(&"=".repeat(title.chars().count().min(WRAP_COLUMNS)));
    out.push('\n');
    if !note.is_empty() {
        out.push('\n');
        out.push_str(note);
        out.push('\n');
    }
    for t in texts {
        out.push('\n');
        out.push_str(t.trim_end());
        out.push('\n');
    }
    out.push_str("\n\n");
}

fn fonts_text() -> String {
    let mut s = String::new();
    block(
        &mut s,
        "Inter (interface font)",
        "Inter Regular and Medium, the typeface used for the interface text.\n\
         Copyright 2020 The Inter Project Authors. SIL Open Font License 1.1.\n\
         https://github.com/rsms/inter",
        &[INTER_LICENSE],
    );
    block(
        &mut s,
        "DejaVu Sans Mono (Text nodes)",
        "Built into the engine as a monospace face for the Text nodes. Based on\n\
         Bitstream Vera; the DejaVu changes are in the public domain.",
        &[DEJAVU_LICENSE],
    );
    block(
        &mut s,
        "Liberation Sans and Liberation Serif (Text nodes)",
        "Built into the engine as the default sans-serif and serif faces for the\n\
         Text nodes. SIL Open Font License 1.1.\n\
         https://github.com/liberationfonts",
        &[LIBERATION_LICENSE],
    );
    block(
        &mut s,
        "Roboto Flex (Text nodes)",
        "Built into the engine as the variable font for the Text nodes. SIL Open\n\
         Font License 1.1.\n\
         https://github.com/googlefonts/roboto-flex",
        &[ROBOTO_FLEX_LICENSE],
    );
    block(
        &mut s,
        "Fonts built into egui",
        "The interface toolkit (egui, through the epaint_default_fonts crate)\n\
         bundles the four fonts below, used as fallbacks for symbols, emoji and\n\
         the monospace text of the code panels.",
        &[],
    );
    block(
        &mut s,
        "Hack (monospace text)",
        "Copyright 2018 Source Foundry Authors, MIT License. Derived from\n\
         Bitstream Vera and DejaVu.\n\
         https://github.com/source-foundry/Hack",
        &[HACK_LICENSE],
    );
    block(
        &mut s,
        "Noto Emoji (emoji)",
        "Copyright The Noto Project Authors. SIL Open Font License 1.1.\n\
         https://github.com/googlefonts/noto-emoji",
        &[NOTO_EMOJI_LICENSE],
    );
    block(
        &mut s,
        "emoji-icon-font (symbols)",
        "Copyright 2014 John Slegers, MIT License.\n\
         https://github.com/jslegers/emoji-icon-font",
        &[EMOJI_ICON_FONT_LICENSE],
    );
    block(
        &mut s,
        "Ubuntu Light (fallback text)",
        "Copyright Canonical Ltd, Ubuntu Font Licence 1.0.\n\
         https://design.ubuntu.com/font",
        &[UBUNTU_LICENSE],
    );
    plain(&s)
}

fn icons_text() -> String {
    let mut s = String::new();
    block(
        &mut s,
        "Phosphor Icons",
        "The icons in the toolbar, headers and status bar are the Phosphor icon\n\
         font (Regular weight), compiled in through the egui-phosphor crate.\n\
         Phosphor Icons are MIT licensed. https://github.com/phosphor-icons/core",
        &[PHOSPHOR_LICENSE],
    );
    block(
        &mut s,
        "egui-phosphor",
        "The crate that packages the font for egui. MIT OR Apache-2.0, used under\n\
         MIT. https://github.com/amPerl/egui-phosphor",
        &[EGUI_PHOSPHOR_LICENSE],
    );
    plain(&s)
}

fn tracking_text() -> String {
    let mut s = String::new();
    block(
        &mut s,
        "MediaPipe landmarker models",
        "Floret's Landmarks node tracks hands, faces and poses with Google\n\
         MediaPipe models. The three model files (hand_landmarker.task,\n\
         face_landmarker.task, pose_landmarker_lite.task) are embedded in the\n\
         app unmodified. They are licensed under the Apache License 2.0.\n\
         https://github.com/google-ai-edge/mediapipe",
        &[MEDIAPIPE_NOTICE],
    );
    block(&mut s, "Apache License 2.0", "", &[MEDIAPIPE_LICENSE]);
    plain(&s)
}

fn crates_text() -> String {
    let rows = crate_rows(CRATES_TABLE);
    let name_w = rows
        .iter()
        .map(|r| r.name.len() + 1 + r.version.len())
        .max()
        .unwrap_or(0);
    let mut s = String::new();
    s.push_str(&format!(
        "RUST CRATES\n===========\n\n\
         {} open-source libraries are compiled into Floret. Where a crate offers a\n\
         choice of licences (OR), Floret uses it under the permissive option. The\n\
         licence texts follow the list.\n\n",
        rows.len()
    ));
    for r in &rows {
        let nv = format!("{} {}", r.name, r.version);
        s.push_str(&format!("{nv:<name_w$}  {}\n", r.licence));
    }
    s.push_str("\n\n");
    s.push_str(CRATE_LICENSE_TEXTS);
    s
}

/// Tidy text for the pane: drop Markdown emphasis and code ticks, and turn
/// `[text](target)` links into `text` (with the address after it for web
/// links).
fn plain(text: &str) -> String {
    let text = text.replace("**", "").replace('`', "");
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let link = after.find("](").and_then(|mid| {
            let target = &after[mid + 2..];
            let end = target.find(')')?;
            let label = &after[..mid];
            // A label that spans a line break isn't a link.
            (!label.contains('\n') && !target[..end].contains('\n'))
                .then(|| (label, &target[..end], open + 1 + mid + 2 + end + 1))
        });
        match link {
            Some((label, target, used)) => {
                out.push_str(&rest[..open]);
                out.push_str(label);
                if target.starts_with("http") && target != label {
                    out.push_str(" (");
                    out.push_str(target);
                    out.push(')');
                }
                rest = &rest[used..];
            }
            None => {
                out.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// One row of `native/licenses/crates.md`.
#[derive(Debug, PartialEq, Eq)]
pub struct CrateRow<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub licence: &'a str,
}

/// The rows of the generated Markdown table.
pub fn crate_rows(table: &str) -> Vec<CrateRow<'_>> {
    table
        .lines()
        .filter_map(|line| {
            let inner = line.strip_prefix("| ")?.strip_suffix(" |")?;
            let mut cells = inner.split(" | ");
            let (name, version, licence) = (cells.next()?, cells.next()?, cells.next()?);
            // The header and the `| --- |` rule.
            if name == "Crate" || name.starts_with("---") {
                return None;
            }
            Some(CrateRow {
                name,
                version,
                licence,
            })
        })
        .collect()
}

/// Wrap `text` at [`WRAP_COLUMNS`] columns, keeping each line's indent.
fn wrap_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.chars().count() <= WRAP_COLUMNS {
            out.push(line.to_owned());
            continue;
        }
        let indent: String = line.chars().take_while(|c| *c == ' ').collect();
        let mut cur = String::new();
        let mut cur_len = 0;
        for word in line.split_whitespace() {
            let wl = word.chars().count();
            if cur_len > 0 && cur_len + 1 + wl > WRAP_COLUMNS {
                out.push(std::mem::take(&mut cur));
                cur_len = 0;
            }
            if cur_len == 0 {
                cur.push_str(&indent);
                cur_len = indent.chars().count();
            } else {
                cur.push(' ');
                cur_len += 1;
            }
            // A word longer than a line (a URL): break it.
            let mut word = word;
            while cur_len + word.chars().count() > WRAP_COLUMNS
                && word.chars().count() > WRAP_COLUMNS / 2
            {
                let room = WRAP_COLUMNS.saturating_sub(cur_len).max(1);
                let cut = word.char_indices().nth(room).map_or(word.len(), |(i, _)| i);
                cur.push_str(&word[..cut]);
                out.push(std::mem::take(&mut cur));
                cur.push_str(&indent);
                cur_len = indent.chars().count();
                word = &word[cut..];
            }
            cur.push_str(word);
            cur_len += word.chars().count();
        }
        out.push(cur);
    }
    out
}

/// The Help → Licenses window.
#[derive(Default)]
pub struct Licenses {
    pub open: bool,
    selected: usize,
    /// Built on first open: the sections and their wrapped lines.
    cache: Option<Vec<Cached>>,
}

struct Cached {
    title: &'static str,
    blurb: &'static str,
    lines: Vec<String>,
}

impl Licenses {
    fn cache(&mut self) -> &[Cached] {
        self.cache.get_or_insert_with(|| {
            sections()
                .into_iter()
                .map(|s| Cached {
                    title: s.title,
                    blurb: s.blurb,
                    lines: wrap_lines(&s.text),
                })
                .collect()
        })
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let mut selected = self.selected;
        let sections = self.cache();
        selected = selected.min(sections.len().saturating_sub(1));
        egui::Window::new("Licenses")
            .open(&mut open)
            .collapsible(false)
            .default_size([940.0, 620.0])
            .min_size([520.0, 260.0])
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                let height = ui.available_height();
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(170.0, height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(170.0);
                            for (i, s) in sections.iter().enumerate() {
                                let text = RichText::new(s.title).color(if i == selected {
                                    *FG_0
                                } else {
                                    *FG_1
                                });
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 24.0],
                                        egui::Button::selectable(i == selected, text),
                                    )
                                    .clicked()
                                {
                                    selected = i;
                                }
                            }
                        },
                    );
                    ui.separator();
                    ui.vertical(|ui| {
                        let s = &sections[selected];
                        ui.label(RichText::new(s.title).color(GOLD_TEXT).strong());
                        ui.label(RichText::new(s.blurb).small().color(FG_2));
                        ui.add_space(4.0);
                        pane(ui, &s.lines, selected, height - 48.0);
                    });
                });
            });
        self.open = open;
        self.selected = selected;
    }
}

/// The text pane: a scroll area that lays out only the visible lines.
fn pane(ui: &mut egui::Ui, lines: &[String], section: usize, height: f32) {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let row_h = ui.fonts_mut(|f| f.row_height(&font));
    egui::Frame::new()
        .fill(BG_1.get())
        .stroke(egui::Stroke::new(1.0, BORDER.get()))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            // No gap between rows, so `show_rows` sizes the list exactly.
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::both()
                .id_salt(("licenses-pane", section))
                .max_height(height.max(120.0))
                .auto_shrink([false, false])
                .show_rows(ui, row_h, lines.len(), |ui, range| {
                    // One label for the rows in view.
                    let text = lines[range].join("\n");
                    ui.add(
                        egui::Label::new(RichText::new(text).font(font).color(FG_0))
                            .extend()
                            .selectable(true),
                    );
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Licences the dependencies may be under. A crate that offers a choice
    /// (`OR`) only needs one allowed alternative; `AND` needs all of them.
    /// LGPL, GPL, AGPL and non-commercial licences are deliberately absent.
    const ALLOWED: &[&str] = &[
        "MIT",
        "Apache-2.0",
        "BSD-2-Clause",
        "BSD-3-Clause",
        "ISC",
        "Zlib",
        "MPL-2.0",
        "Unicode-3.0",
        "Unicode-DFS-2016",
        "CDLA-Permissive-2.0",
        "OFL-1.1",
        "BSL-1.0",
        "0BSD",
        "CC0-1.0",
        "Unlicense",
        // The Ubuntu Light font that egui bundles (`epaint_default_fonts`
        // is `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`): a free
        // font licence that allows bundling the font with software.
        "Ubuntu-font-1.0",
    ];
    /// Exceptions that may follow `WITH`.
    const EXCEPTIONS: &[&str] = &["LLVM-exception"];

    #[derive(Debug)]
    enum Expr {
        Id {
            id: String,
            exception: Option<String>,
        },
        And(Vec<Expr>),
        Or(Vec<Expr>),
    }

    impl Expr {
        fn allowed(&self) -> bool {
            match self {
                Expr::Id { id, exception } => {
                    ALLOWED.contains(&id.as_str())
                        && exception.as_deref().is_none_or(|e| EXCEPTIONS.contains(&e))
                }
                Expr::And(v) => v.iter().all(Expr::allowed),
                Expr::Or(v) => v.iter().any(Expr::allowed),
            }
        }
    }

    /// Parse an SPDX expression (`/` counts as `OR`, as old crates write it).
    fn parse(expr: &str) -> Result<Expr, String> {
        let mut tokens = Vec::new();
        let mut cur = String::new();
        for c in expr.chars() {
            if c.is_whitespace() || "()/".contains(c) {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
                if !c.is_whitespace() {
                    tokens.push(c.to_string());
                }
            } else {
                cur.push(c);
            }
        }
        if !cur.is_empty() {
            tokens.push(cur);
        }
        fn or(t: &[String], p: &mut usize) -> Result<Expr, String> {
            let mut parts = vec![and(t, p)?];
            while t
                .get(*p)
                .is_some_and(|x| x.eq_ignore_ascii_case("or") || x == "/")
            {
                *p += 1;
                parts.push(and(t, p)?);
            }
            Ok(if parts.len() == 1 {
                parts.remove(0)
            } else {
                Expr::Or(parts)
            })
        }
        fn and(t: &[String], p: &mut usize) -> Result<Expr, String> {
            let mut parts = vec![atom(t, p)?];
            while t.get(*p).is_some_and(|x| x.eq_ignore_ascii_case("and")) {
                *p += 1;
                parts.push(atom(t, p)?);
            }
            Ok(if parts.len() == 1 {
                parts.remove(0)
            } else {
                Expr::And(parts)
            })
        }
        fn atom(t: &[String], p: &mut usize) -> Result<Expr, String> {
            let tok = t.get(*p).ok_or("expression ends early")?.clone();
            *p += 1;
            if tok == "(" {
                let e = or(t, p)?;
                if t.get(*p).map(String::as_str) != Some(")") {
                    return Err("missing )".into());
                }
                *p += 1;
                return Ok(e);
            }
            if [")", "/"].contains(&tok.as_str())
                || ["or", "and", "with"].contains(&tok.to_lowercase().as_str())
            {
                return Err(format!("unexpected {tok}"));
            }
            let mut exception = None;
            if t.get(*p).is_some_and(|x| x.eq_ignore_ascii_case("with")) {
                *p += 1;
                exception = Some(t.get(*p).ok_or("WITH needs an exception")?.clone());
                *p += 1;
            }
            Ok(Expr::Id { id: tok, exception })
        }
        let mut p = 0;
        let e = or(&tokens, &mut p)?;
        if p != tokens.len() {
            return Err(format!("unexpected {}", tokens[p]));
        }
        Ok(e)
    }

    /// The `(name, version)` of every package in `Cargo.lock` that comes
    /// from a registry (the workspace's own crates have no `source`).
    fn lock_packages() -> BTreeSet<(String, String)> {
        let lock = include_str!("../../../Cargo.lock");
        let mut out = BTreeSet::new();
        for block in lock.split("[[package]]").skip(1) {
            let field = |key: &str| {
                block.lines().find_map(|l| {
                    l.strip_prefix(key)
                        .and_then(|r| r.strip_prefix(" = \""))
                        .and_then(|r| r.strip_suffix('"'))
                })
            };
            let (Some(name), Some(version)) = (field("name"), field("version")) else {
                continue;
            };
            if field("source").is_some() {
                out.insert((name.to_owned(), version.to_owned()));
            }
        }
        out
    }

    #[test]
    fn every_locked_crate_is_listed_with_its_version() {
        let lock = lock_packages();
        assert!(lock.len() > 100, "Cargo.lock parsed wrongly");
        let listed: BTreeSet<(String, String)> = crate_rows(CRATES_TABLE)
            .iter()
            .map(|r| (r.name.to_owned(), r.version.to_owned()))
            .collect();
        let missing: Vec<String> = lock
            .difference(&listed)
            .map(|(n, v)| format!("{n} {v}"))
            .collect();
        let stale: Vec<String> = listed
            .difference(&lock)
            .map(|(n, v)| format!("{n} {v}"))
            .collect();
        assert!(
            missing.is_empty() && stale.is_empty(),
            "native/licenses/crates.md is out of date; run `python3 -I tools/gen-licenses.py`.\n\
             in Cargo.lock but not listed: {missing:?}\nlisted but not in Cargo.lock: {stale:?}"
        );
    }

    #[test]
    fn every_listed_licence_is_on_the_allow_list() {
        let rows = crate_rows(CRATES_TABLE);
        assert!(rows.len() > 100);
        let mut bad = Vec::new();
        for r in &rows {
            match parse(r.licence) {
                Ok(e) if e.allowed() => {}
                Ok(_) => bad.push(format!("{} {}: {}", r.name, r.version, r.licence)),
                Err(err) => bad.push(format!(
                    "{} {}: unreadable licence `{}` ({err})",
                    r.name, r.version, r.licence
                )),
            }
        }
        assert!(
            bad.is_empty(),
            "dependencies under a licence outside the allow-list:\n  {}",
            bad.join("\n  ")
        );
    }

    #[test]
    fn allow_list_check_reads_spdx_expressions() {
        let ok = |s: &str| parse(s).unwrap().allowed();
        assert!(ok("MIT"));
        assert!(ok("MIT OR Apache-2.0"));
        assert!(ok("Apache-2.0/MIT"));
        assert!(ok("Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT"));
        assert!(ok("(MIT OR Apache-2.0) AND Unicode-3.0"));
        // A copyleft alternative is fine while a permissive one is offered.
        assert!(ok("MIT OR Apache-2.0 OR LGPL-2.1-or-later"));
        assert!(!ok("GPL-3.0-only"));
        assert!(!ok("MIT AND LGPL-2.1-or-later"));
        assert!(!ok("Apache-2.0 WITH Classpath-exception-2.0"));
        assert!(!ok("CC-BY-NC-4.0"));
        assert!(parse("MIT OR").is_err());
    }

    #[test]
    fn the_list_names_the_libraries_called_out_in_the_notices() {
        let names: BTreeSet<&str> = crate_rows(CRATES_TABLE).iter().map(|r| r.name).collect();
        for n in [
            "egui",
            "wgpu",
            "delaunator",
            "robust",
            "i_overlay",
            "libm",
            "gif",
            "color_quant",
            "weezl",
            "egui-phosphor",
        ] {
            assert!(names.contains(n), "{n} missing from crates.md");
        }
        // Floret's own crates are not third-party.
        assert!(!names.iter().any(|n| n.starts_with("floret-")));
    }

    #[test]
    fn sections_are_listed_and_have_text() {
        let all = sections();
        let titles: Vec<_> = all.iter().map(|s| s.title).collect();
        assert_eq!(
            titles,
            [
                "Floret",
                "Fonts",
                "Icons",
                "Tracking models",
                "Rust crates",
                "Notices"
            ]
        );
        for s in &all {
            assert!(s.text.trim().len() > 200, "{} has no text", s.title);
            assert!(!s.blurb.is_empty(), "{} has no blurb", s.title);
        }
        let text_of = |t: &str| &all.iter().find(|s| s.title == t).unwrap().text;
        assert!(text_of("Floret").contains("Jaafer Elsadig"));
        for font in [
            "Inter",
            "DejaVu",
            "Liberation",
            "Roboto Flex",
            "Hack",
            "Noto Emoji",
            "Ubuntu",
        ] {
            assert!(text_of("Fonts").contains(font), "{font} missing");
        }
        assert!(text_of("Icons").contains("Phosphor"));
        assert!(text_of("Tracking models").contains("Apache License"));
        assert!(text_of("Rust crates").contains("Apache License"));
        assert!(text_of("Notices").contains("Third-Party Notices"));
    }

    #[test]
    fn markdown_links_read_as_plain_text() {
        assert_eq!(
            plain(
                "see [`LICENSE`](LICENSE), **bold** and [the site](https://x.org/a) [not a link]"
            ),
            "see LICENSE, bold and the site (https://x.org/a) [not a link]"
        );
        assert_eq!(plain("[https://a.b](https://a.b)"), "https://a.b");
        assert_eq!(plain("a [b\n](c) d"), "a [b\n](c) d");
    }

    #[test]
    fn wrapping_keeps_every_word_and_the_width() {
        let long = format!("    {}", "word ".repeat(60));
        let wrapped = wrap_lines(&format!("short\n{long}\n{}", "x".repeat(300)));
        assert_eq!(wrapped[0], "short");
        assert!(wrapped.iter().all(|l| l.chars().count() <= WRAP_COLUMNS));
        let words = |s: &str| s.split_whitespace().collect::<String>();
        assert_eq!(
            words(&wrapped[1..wrapped.len() - 4].join(" ")),
            words(&long)
        );
        // The unbroken run is cut into pieces with nothing lost.
        let x: usize = wrapped.iter().map(|l| l.matches('x').count()).sum();
        assert_eq!(x, 300);
    }

    #[test]
    fn the_window_draws_every_section_headless() {
        let ctx = egui::Context::default();
        let mut w = Licenses {
            open: true,
            ..Licenses::default()
        };
        for i in 0..sections().len() {
            w.selected = i;
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 800.0),
                )),
                ..Default::default()
            };
            ctx.run_ui(raw, |ui| w.show(ui.ctx()))
                .textures_delta
                .clear();
            assert!(w.open);
            assert_eq!(w.selected, i);
        }
    }
}
