//! Inline port editing and value display: which rows a node shows and
//! which editor each gets (`GenericNode.tsx`), scrub maths
//! (`ScrubInput.tsx`), the floating port-value editor's parsing
//! (`PortValueEditor.tsx`), tooltip formatting (`ValueTooltip.tsx`), and
//! pushing live edits into a running engine without a recompile.

use super::ports::find_node_mut;
use floret_engine::patch::{PatchFile, PatchNode};
use floret_engine::spec::{Catalogue, NodeSpec, PortSpec, PortType};
use floret_engine::{Engine, Spread, Value};
use serde_json::{json, Value as Json};

/// Where an inline edit is stored.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum EditKey {
    /// A node param (`PortSpec.param`, or a pure param row): live through
    /// the engine's param store, no recompile.
    Param(String),
    /// `portValues[port]`: compiled into the node's input source.
    Port(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldKind {
    Number { integer: bool },
    Vec2,
    Bool,
    Text,
    Color,
    Select(Vec<String>),
}

/// One left-column row of a generic node.
#[derive(Clone, Debug)]
pub enum LeftItem {
    /// A param with no pin (always shows its editor).
    Param {
        key: String,
        kind: FieldKind,
    },
    Input(PortSpec),
}

/// Pure params worth a row on the node: dropdowns and Read / Write slots.
/// (The web app also lists bookkeeping params like `valueType`; those stay
/// in the inspector here.)
fn param_row_kind(spec: &NodeSpec, key: &str, default: &Json) -> Option<FieldKind> {
    if let Some(opts) = spec.param_options.get(key) {
        return Some(FieldKind::Select(opts.clone()));
    }
    (key == "slot" && default.is_number()).then_some(FieldKind::Number { integer: true })
}

/// Left column of a generic node: pure-param rows, then inputs.
pub fn left_items(cat: &Catalogue, spec: &NodeSpec, n: &PatchNode) -> Vec<LeftItem> {
    let inputs = cat.resolved_inputs(spec, &n.params);
    let mut items: Vec<LeftItem> = spec
        .default_params
        .iter()
        .filter(|(k, _)| {
            !inputs
                .iter()
                .any(|p| p.param.as_deref() == Some(k.as_str()))
        })
        .filter_map(|(k, d)| {
            param_row_kind(spec, k, d).map(|kind| LeftItem::Param {
                key: k.clone(),
                kind,
            })
        })
        .collect();
    items.extend(inputs.into_iter().map(LeftItem::Input));
    items
}

/// The inline editor for an input (`editorMode`): none when wired or the
/// type has no editor.
pub fn input_editor(spec: &NodeSpec, port: &PortSpec, wired: bool) -> Option<(EditKey, FieldKind)> {
    if wired {
        return None;
    }
    let key = match &port.param {
        Some(p) => EditKey::Param(p.clone()),
        None => EditKey::Port(port.name.clone()),
    };
    if let Some(opts) = port.param.as_ref().and_then(|p| spec.param_options.get(p)) {
        return Some((key, FieldKind::Select(opts.clone())));
    }
    let kind = match port.ty {
        PortType::Number => FieldKind::Number {
            integer: port.integer,
        },
        PortType::Boolean => FieldKind::Bool,
        PortType::String => FieldKind::Text,
        PortType::Vec2 => FieldKind::Vec2,
        PortType::Color => FieldKind::Color,
        _ => return None,
    };
    Some((key, kind))
}

/// A value as `.flora` JSON (what params / portValues hold).
pub fn value_to_json(v: &Value) -> Json {
    match v {
        Value::Num(n) => json!(n),
        Value::Bool(b) => json!(b),
        Value::Str(s) => json!(s.as_ref()),
        Value::Vec2(p) => json!({ "x": p.x, "y": p.y }),
        Value::Color(c) => json!({ "r": c.r, "g": c.g, "b": c.b, "a": c.a }),
        _ => Json::Null,
    }
}

/// The value an editor shows: the stored param / port value, else the
/// declared default.
pub fn current(n: &PatchNode, spec: &NodeSpec, key: &EditKey, port: Option<&PortSpec>) -> Json {
    let stored = match key {
        EditKey::Param(k) => n.params.get(k).or_else(|| spec.default_params.get(k)),
        EditKey::Port(p) => n.port_values.as_ref().and_then(|pv| pv.get(p)),
    };
    if let Some(v) = stored {
        return v.clone();
    }
    port.and_then(|p| p.default.as_ref())
        .map(value_to_json)
        .unwrap_or(Json::Null)
}

/// Write an edit into the patch. Returns whether the value changed.
pub fn set(patch: &mut PatchFile, node: &str, key: &EditKey, v: Json) -> bool {
    let Some(n) = find_node_mut(patch, node) else {
        return false;
    };
    let slot = match key {
        EditKey::Param(k) => n.params.entry(k.clone()),
        EditKey::Port(p) => n
            .port_values
            .get_or_insert_with(Default::default)
            .entry(p.clone()),
    };
    match slot {
        serde_json::map::Entry::Occupied(mut o) => {
            if o.get() == &v {
                return false;
            }
            o.insert(v);
        }
        serde_json::map::Entry::Vacant(e) => {
            e.insert(v);
        }
    }
    true
}

/// `clearNodePortValue`.
pub fn clear_port_value(patch: &mut PatchFile, node: &str, port: &str) -> bool {
    let Some(pv) = find_node_mut(patch, node).and_then(|n| n.port_values.as_mut()) else {
        return false;
    };
    pv.remove(port).is_some()
}

/// After a param reshaped a node's inputs (`setNodeParam`), drop wires
/// into inputs that no longer exist.
pub fn prune_dead_inputs(patch: &mut PatchFile, cat: &Catalogue, node: &str) {
    let Some(names) = patch
        .nodes
        .iter()
        .find(|n| n.id == node)
        .and_then(|n| {
            cat.get(&n.def_type)
                .map(|s| cat.resolved_inputs(s, &n.params))
        })
        .map(|ins| ins.into_iter().map(|p| p.name).collect::<Vec<_>>())
    else {
        return;
    };
    patch.edges.retain(|e| {
        e.target != node
            || match super::ports::split_handle(e.target_handle.as_deref()) {
                Some((super::ports::Side::In, name)) => names.iter().any(|n| n == name),
                _ => true,
            }
    });
}

pub fn as_f64(v: &Json) -> f64 {
    match v {
        Json::Number(n) => n.as_f64().unwrap_or(0.0),
        Json::Bool(b) => f64::from(u8::from(*b)),
        Json::String(s) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

pub fn as_vec2(v: &Json) -> (f64, f64) {
    let f = |k: &str| v.get(k).map(as_f64).unwrap_or(0.0);
    (f("x"), f("y"))
}

pub fn as_bool(v: &Json) -> bool {
    match v {
        Json::Bool(b) => *b,
        Json::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Json::String(s) => s == "true",
        _ => false,
    }
}

pub fn as_text(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Null => String::new(),
        other => other.to_string(),
    }
}

/// Colour channels 0..1 from `{r,g,b,a}` (or a `#rrggbb` string).
pub fn as_rgba(v: &Json) -> [f64; 4] {
    if let Json::String(s) = v {
        return parse_hex(s).unwrap_or([0.0, 0.0, 0.0, 1.0]);
    }
    let f = |k: &str, d: f64| v.get(k).map(as_f64).unwrap_or(d);
    [f("r", 0.0), f("g", 0.0), f("b", 0.0), f("a", 1.0)]
}

pub fn rgba_json([r, g, b, a]: [f64; 4]) -> Json {
    json!({ "r": r, "g": g, "b": b, "a": a })
}

/// `#rrggbb`, or `#rrggbbaa` when not opaque.
pub fn hex([r, g, b, a]: [f64; 4]) -> String {
    let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    if c(a) == 255 {
        format!("#{:02x}{:02x}{:02x}", c(r), c(g), c(b))
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", c(r), c(g), c(b), c(a))
    }
}

/// Parse `#rgb`, `#rrggbb` or `#rrggbbaa` (the `#` optional).
pub fn parse_hex(s: &str) -> Option<[f64; 4]> {
    let h = s.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    let n = |b: u8| f64::from(b) / 255.0;
    match h.len() {
        3 => {
            let d = |i: usize| {
                u8::from_str_radix(h.get(i..i + 1)?, 16)
                    .ok()
                    .map(|v| v * 17)
            };
            Some([n(d(0)?), n(d(1)?), n(d(2)?), 1.0])
        }
        6 => Some([n(byte(0)?), n(byte(2)?), n(byte(4)?), 1.0]),
        8 => Some([n(byte(0)?), n(byte(2)?), n(byte(4)?), n(byte(6)?)]),
        _ => None,
    }
}

const PX_PER_STEP: f64 = 4.0;

/// `ScrubInput`: `start` dragged `dx` px; Shift ×10, Alt ×0.1.
pub fn scrub(start: f64, dx: f64, shift: bool, alt: bool, integer: bool) -> f64 {
    let m = if shift {
        10.0
    } else if alt {
        0.1
    } else {
        1.0
    };
    let v = start + (dx / PX_PER_STEP) * m;
    if integer {
        v.round()
    } else {
        // Keep scrubbed values tidy (the TS field shows 3 decimals).
        (v * 1000.0).round() / 1000.0
    }
}

/// Parse typed text for a field. `None` = invalid (keep the old value).
pub fn parse_field(kind: &FieldKind, text: &str) -> Option<Json> {
    let t = text.trim();
    match kind {
        FieldKind::Number { integer } => {
            let v: f64 = t.parse().ok().filter(|v: &f64| v.is_finite())?;
            Some(json!(if *integer { v.round() } else { v }))
        }
        FieldKind::Text | FieldKind::Select(_) => Some(json!(text)),
        FieldKind::Bool => Some(json!(t == "true" || t == "1")),
        FieldKind::Color => parse_hex(t).map(rgba_json),
        FieldKind::Vec2 => {
            let mut it = t.split([',', ' ']).filter(|s| !s.is_empty());
            let x: f64 = it.next()?.parse().ok()?;
            let y: f64 = it.next()?.parse().ok()?;
            Some(json!({ "x": x, "y": y }))
        }
    }
}

/// The floating port editor (`PortValueEditor` commit): `Some(None)` =
/// clear the override, `Some(Some(v))` = set, `None` = invalid.
pub fn port_editor_value(ty: PortType, raw: &str) -> Option<Option<Json>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Some(None);
    }
    let v = match ty {
        PortType::Number => json!(raw.parse::<f64>().ok().filter(|v| v.is_finite())?),
        PortType::Boolean => json!(raw == "true" || raw == "1"),
        PortType::Color => rgba_json(parse_hex(raw)?),
        PortType::Vec2 => parse_field(&FieldKind::Vec2, raw)?,
        _ => json!(raw),
    };
    Some(Some(v))
}

/// The text the floating editor opens with.
pub fn port_editor_text(v: &Json) -> String {
    match v {
        Json::Null => String::new(),
        Json::Number(_) => num(as_f64(v), 6),
        Json::Object(_) if v.get("r").is_some() => hex(as_rgba(v)),
        Json::Object(_) => {
            let (x, y) = as_vec2(v);
            format!("{}, {}", num(x, 3), num(y, 3))
        }
        other => as_text(other),
    }
}

/// A number with up to `decimals` places, trailing zeros trimmed.
pub fn num(v: f64, decimals: usize) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{v:.0}");
    }
    let s = format!("{v:.decimals$}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_owned()
    }
}

/// GenericNode `formatValue`: the compact readout next to a label.
pub fn short(v: &Json) -> String {
    match v {
        Json::Number(_) => num(as_f64(v), 2),
        Json::Bool(b) => b.to_string(),
        Json::Null => "—".into(),
        Json::Object(_) if v.get("r").is_some() => hex(as_rgba(v)),
        Json::Object(_) => {
            let (x, y) = as_vec2(v);
            format!("{}, {}", num(x, 2), num(y, 2))
        }
        Json::String(s) if s.chars().count() > 10 => {
            format!("{}…", s.chars().take(9).collect::<String>())
        }
        other => as_text(other),
    }
}

fn tooltip_value(v: &Value) -> String {
    match v {
        Value::Undefined => "—".into(),
        Value::Null => "null".into(),
        Value::Num(n) => num(*n, 3),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => format!("\"{s}\""),
        Value::Vec2(p) => format!("vec2({}, {})", num(p.x, 3), num(p.y, 3)),
        Value::Color(c) => hex([c.r, c.g, c.b, 1.0]),
        Value::List(items) if items.is_empty() => "[ ]".into(),
        Value::List(items) => {
            let head: Vec<String> = items.iter().take(3).map(tooltip_value).collect();
            let more = if items.len() > 3 {
                format!(", …×{}", items.len())
            } else {
                String::new()
            };
            format!("[{}{more}]", head.join(", "))
        }
        Value::Object(_) => "{…}".into(),
        Value::Scene(n) => n.kind().to_owned(),
        Value::Style(_) => "style".into(),
        Value::Handle(h) => h.type_name().to_owned(),
        Value::Image(img) => format!("image {}×{}", img.width(), img.height()),
    }
}

/// `ValueTooltip`: a spread shows its head and `(1 of N)`.
pub fn tooltip(s: &Spread) -> String {
    match s.len() {
        0 => "[ ]".into(),
        1 => tooltip_value(&s[0]),
        n => format!("{}  (1 of {n})", tooltip_value(&s[0])),
    }
}

/// A pin's display label (`withCanonicalLabels`): an explicit label wins;
/// otherwise generic number outputs read "value", audio inputs "audio" and
/// a node's main layer output "layer". Handle ids are untouched.
pub fn display_label(p: &PortSpec, input: bool) -> String {
    if let Some(l) = &p.label {
        return l.clone();
    }
    let canonical =
        if !input && matches!(p.name.as_str(), "result" | "number") && p.ty != PortType::Boolean {
            Some("value")
        } else if input && p.ty == PortType::Audio {
            Some("audio")
        } else if !input
            && p.ty == PortType::Layer
            && p.kind != floret_engine::spec::PortKind::Spread
            && !matches!(p.name.as_str(), "viz" | "guides" | "inner" | "outer")
        {
            Some("layer")
        } else {
            None
        };
    canonical.map_or_else(|| p.name.clone(), str::to_owned)
}

/// Header symbols for math nodes (`MATH_SYMBOLS`).
pub fn math_symbol(ty: &str) -> Option<&'static str> {
    Some(match ty {
        "math/add" => "+",
        "math/sub" => "−",
        "math/mul" => "×",
        "math/div" => "÷",
        "math/mod" => "mod",
        "math/pow" => "x^y",
        "math/neg" => "−x",
        "math/abs" => "|x|",
        "math/floor" => "⌊x⌋",
        "math/ceil" => "⌈x⌉",
        "math/round" => "⌊x⌉",
        "math/fract" => "{x}",
        "math/sqrt" => "√",
        "math/sq" => "x²",
        "math/exp" => "e^x",
        "math/log" => "ln",
        "math/sin" => "sin",
        "math/cos" => "cos",
        "math/tan" => "tan",
        "math/asin" => "asin",
        "math/acos" => "acos",
        "math/atan" => "atan",
        "math/atan2" => "atan2",
        "math/min" => "min",
        "math/max" => "max",
        "math/mag" => "‖v‖",
        "math/dist" => "dist",
        _ => return None,
    })
}

/// An edit to push into the running engine this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum LiveEdit {
    Param {
        node: String,
        key: String,
        value: Json,
    },
    Port {
        node: String,
        port: String,
        value: Json,
    },
}

/// Apply live edits without recompiling: params go to the engine's param
/// store; port values patch the compiled input source in place.
pub fn apply_live(engine: &mut Engine, cat: &Catalogue, patch: &PatchFile, edits: Vec<LiveEdit>) {
    for e in edits {
        match e {
            LiveEdit::Param { node, key, value } => engine.params.set(&node, &key, value),
            LiveEdit::Port { node, port, value } => {
                let Some(n) = patch.nodes.iter().find(|n| n.id == node) else {
                    continue;
                };
                let Some(spec) = cat.get(&n.def_type) else {
                    continue;
                };
                let Some(k) = cat
                    .resolved_inputs(spec, &n.params)
                    .iter()
                    .position(|p| p.name == port)
                else {
                    continue;
                };
                let Some(idx) = engine.graph.idx(&node) else {
                    continue;
                };
                if let Some(src) = engine.graph.nodes[idx]
                    .sources
                    .as_mut()
                    .and_then(|s| s.get_mut(k))
                {
                    src.port_value = Some(Value::from_json(&value));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_util::{node, patch};
    use floret_engine::value::{Color, Vec2};

    #[test]
    fn rows_put_dropdown_params_first() {
        let cat = Catalogue::builtin();
        let spec = cat.get("color/color").unwrap();
        let mut n = node("c", "color/color");
        n.params = spec.default_params.clone();
        let items = left_items(cat, spec, &n);
        assert!(
            matches!(&items[0], LeftItem::Param { key, kind: FieldKind::Select(o) } if key == "mode" && o.len() == 3)
        );
        assert!(items
            .iter()
            .skip(1)
            .all(|i| matches!(i, LeftItem::Input(_))));
        // Bookkeeping params get no row.
        let add = cat.get("math/add").unwrap();
        assert!(left_items(cat, add, &node("a", "math/add"))
            .iter()
            .all(|i| matches!(i, LeftItem::Input(_))));
    }

    #[test]
    fn editors_follow_the_port() {
        let cat = Catalogue::builtin();
        let spec = cat.get("vec2/make").unwrap();
        let x = &spec.inputs[0];
        let (key, kind) = input_editor(spec, x, false).unwrap();
        assert!(matches!(kind, FieldKind::Number { .. }));
        assert_eq!(
            key,
            match &x.param {
                Some(p) => EditKey::Param(p.clone()),
                None => EditKey::Port(x.name.clone()),
            }
        );
        assert!(
            input_editor(spec, x, true).is_none(),
            "wired inputs don't edit"
        );
    }

    #[test]
    fn set_and_clear_round_trip() {
        let mut p = patch(vec![node("n", "math/add")], vec![]);
        let k = EditKey::Port("a".into());
        assert!(set(&mut p, "n", &k, json!(2.5)));
        assert!(!set(&mut p, "n", &k, json!(2.5)), "unchanged");
        assert_eq!(p.nodes[0].port_values.as_ref().unwrap()["a"], json!(2.5));
        assert!(set(&mut p, "n", &EditKey::Param("b".into()), json!(1)));
        assert_eq!(p.nodes[0].params["b"], json!(1));
        assert!(clear_port_value(&mut p, "n", "a"));
        assert!(!clear_port_value(&mut p, "n", "a"));
    }

    #[test]
    fn scrub_modifiers() {
        assert_eq!(scrub(10.0, 8.0, false, false, false), 12.0);
        assert_eq!(scrub(10.0, 8.0, true, false, false), 30.0);
        assert_eq!(scrub(10.0, 8.0, false, true, false), 10.2);
        assert_eq!(scrub(10.0, 6.0, false, false, true), 12.0);
    }

    #[test]
    fn parsing() {
        assert_eq!(
            parse_field(&FieldKind::Number { integer: false }, " 1.5 "),
            Some(json!(1.5))
        );
        assert_eq!(
            parse_field(&FieldKind::Number { integer: false }, "abc"),
            None
        );
        assert_eq!(
            parse_field(&FieldKind::Vec2, "3, -4"),
            Some(json!({"x": 3.0, "y": -4.0}))
        );
        assert_eq!(parse_hex("#ff0000"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_hex("0f0"), Some([0.0, 1.0, 0.0, 1.0]));
        assert_eq!(parse_hex("#zz0000"), None);
        assert_eq!(hex([1.0, 0.5, 0.0, 1.0]), "#ff8000");
        assert_eq!(port_editor_value(PortType::Number, ""), Some(None));
        assert_eq!(port_editor_value(PortType::Number, "x"), None);
        assert_eq!(
            port_editor_value(PortType::Boolean, "1"),
            Some(Some(json!(true)))
        );
        assert_eq!(port_editor_text(&json!({"x": 1.0, "y": 2.5})), "1, 2.5");
        assert_eq!(port_editor_text(&json!(0.0)), "0");
    }

    #[test]
    fn formatting_matches_the_web_app() {
        assert_eq!(num(1.0, 3), "1");
        assert_eq!(num(0.12345, 3), "0.123");
        assert_eq!(num(2.5, 3), "2.5");
        assert_eq!(short(&json!("a long string here")), "a long st…");
        assert_eq!(short(&json!({"x": 1.234, "y": 2})), "1.23, 2");
        let one: Spread = vec![Value::Vec2(Vec2 { x: 1.0, y: 2.5 })].into();
        assert_eq!(tooltip(&one), "vec2(1, 2.5)");
        let many: Spread = vec![Value::Num(0.5), Value::Num(1.0), Value::Num(2.0)].into();
        assert_eq!(tooltip(&many), "0.5  (1 of 3)");
        let c: Spread = vec![Value::Color(Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        })]
        .into();
        assert_eq!(tooltip(&c), "#ff0000");
        assert_eq!(tooltip(&Spread::from(Vec::new())), "[ ]");
        assert_eq!(math_symbol("math/mul"), Some("×"));
        let cat = Catalogue::builtin();
        let add = cat.get("math/add").unwrap();
        assert_eq!(display_label(&add.outputs[0], false), "value");
        let rect = cat.get("draw/rect").unwrap();
        assert_eq!(display_label(&rect.outputs[0], false), "layer");
        assert_eq!(
            display_label(&rect.inputs[0], true),
            rect.inputs[0]
                .label
                .clone()
                .unwrap_or(rect.inputs[0].name.clone())
        );
        assert_eq!(math_symbol("math/lerp"), None);
    }

    #[test]
    fn live_port_edit_reaches_the_engine_without_recompile() {
        let cat = Catalogue::builtin();
        let mut p = patch(vec![node("m", "vec2/make")], vec![]);
        let (mut engine, _) = Engine::from_patch(&p).unwrap();
        set(&mut p, "m", &EditKey::Port("x".into()), json!(7.0));
        apply_live(
            &mut engine,
            cat,
            &p,
            vec![LiveEdit::Port {
                node: "m".into(),
                port: "x".into(),
                value: json!(7.0),
            }],
        );
        let out = engine.tick(&Default::default()).outputs;
        let i = engine.graph.idx("m").unwrap();
        let v = &out[i].as_ref().unwrap()[0];
        assert!(matches!(v[0], Value::Vec2(Vec2 { x, .. }) if x == 7.0));
    }
}
