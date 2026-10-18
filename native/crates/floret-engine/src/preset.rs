//! Presets: named snapshots of a patch's editable values, saved in the
//! `.flora` file (`PatchFile::presets`), to recall or morph between.
//!
//! A preset holds what the user tunes by hand, not the patch's structure:
//! - every unwired input pin with an inline editor (number, boolean,
//!   string, vec2, colour) — its `portValues` constant, or its param when
//!   the pin is param-backed (a mode dropdown, a seed);
//! - the value params of nodes whose controls aren't pins: Slider /
//!   Stepper / Toggle / XY pad / Number / String / Select values, colour
//!   pickers, palette stops, blend and filter modes …;
//! - the patch seed.
//!
//! Left out: wired pins (the wire wins anyway), and setup / bookkeeping
//! params — anything that reshapes a node's pins (`valueType`, slot counts,
//! a mode that swaps inputs), sizes, labels, files, URLs, devices, shader
//! source and the like ([`SETUP_PARAMS`], [`structural`]). So recalling
//! a preset never rewires the patch.
//!
//! Morphing interpolates numbers, vec2s and colours (also `#rrggbb`
//! strings); integer pins round; everything else (booleans, strings,
//! dropdowns, the seed) snaps from A to B at the midpoint.

use crate::patch::{PatchFile, PatchNode, Preset, PresetValues};
use crate::spec::{Catalogue, NodeSpec, PortSpec, PortType};
use crate::value::Value;
use serde_json::{json, Value as Json};

/// Where a preset value lives on its node.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// `params[key]`.
    Param(String),
    /// `portValues[port]`.
    Port(String),
}

/// One value a preset covers.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub node: String,
    pub key: Key,
    pub value: Json,
    /// An integer pin: morphs round.
    pub integer: bool,
}

/// Params that are setup or bookkeeping rather than values to vary.
pub const SETUP_PARAMS: &[&str] = &[
    "valueType",
    "slotCount",
    "stopCount",
    "slot",
    // A Write / Read pair's name.
    "name",
    "label",
    "width",
    "height",
    "dataUrl",
    "mimeType",
    "filename",
    "source",
    "markup",
    "data",
    "url",
    "options",
    "device",
    "tracker",
];

/// Further params left out when they aren't behind a pin: comment text,
/// slider / stepper ranges, device and analysis settings.
const SETUP_PURE_PARAMS: &[&str] = &[
    "text",
    "min",
    "max",
    "step",
    "count",
    "feedback",
    "channel",
    "cc",
    "person",
    "mirror",
    "bins",
    "oversample",
    "edges",
];

/// Every value a preset of this patch holds, with its current value, in
/// patch order.
pub fn fields(cat: &Catalogue, patch: &PatchFile) -> Vec<Field> {
    let mut out = Vec::new();
    for n in &patch.nodes {
        if let Some(spec) = cat.get(&n.def_type) {
            node_fields(cat, spec, patch, n, &mut out);
        }
    }
    out
}

fn node_fields(
    cat: &Catalogue,
    spec: &NodeSpec,
    patch: &PatchFile,
    n: &PatchNode,
    out: &mut Vec<Field>,
) {
    let inputs = cat.resolved_inputs(spec, &n.params);
    let wired = |port: &str| {
        patch.edges.iter().any(|e| {
            e.target == n.id
                && e.target_handle
                    .as_deref()
                    .and_then(|h| h.strip_prefix("in-"))
                    == Some(port)
        })
    };
    let mut push = |key: Key, value: Option<Json>, integer: bool| {
        if let Some(value) = value.filter(presettable) {
            out.push(Field {
                node: n.id.clone(),
                key,
                value,
                integer,
            });
        }
    };
    // Pure params (no pin): what the node's own controls edit.
    for (k, default) in &spec.default_params {
        if inputs
            .iter()
            .any(|p| p.param.as_deref() == Some(k.as_str()))
            || SETUP_PARAMS.contains(&k.as_str())
            || SETUP_PURE_PARAMS.contains(&k.as_str())
            || k.starts_with("__")
            || structural(cat, spec, n, k)
        {
            continue;
        }
        push(
            Key::Param(k.clone()),
            Some(n.params.get(k).unwrap_or(default).clone()),
            false,
        );
    }
    // Unwired pins with an inline editor.
    for p in &inputs {
        if !editable(p) || wired(&p.name) {
            continue;
        }
        match &p.param {
            Some(k) => {
                if SETUP_PARAMS.contains(&k.as_str()) || structural(cat, spec, n, k) {
                    continue;
                }
                let v = n
                    .params
                    .get(k)
                    .or_else(|| spec.default_params.get(k))
                    .cloned();
                push(
                    Key::Param(k.clone()),
                    v.or_else(|| default_json(p)),
                    p.integer,
                );
            }
            None => {
                let v = n
                    .port_values
                    .as_ref()
                    .and_then(|pv| pv.get(&p.name))
                    .cloned();
                push(
                    Key::Port(p.name.clone()),
                    v.or_else(|| default_json(p)),
                    p.integer,
                );
            }
        }
    }
}

/// A pin type with an inline editor.
fn editable(p: &PortSpec) -> bool {
    matches!(
        p.ty,
        PortType::Number | PortType::Boolean | PortType::String | PortType::Vec2 | PortType::Color
    )
}

/// A value a preset can hold: a number, boolean, string, vec2 or colour.
fn presettable(v: &Json) -> bool {
    match v {
        Json::Number(_) | Json::Bool(_) | Json::String(_) => true,
        Json::Object(_) => as_vec2(v).is_some() || as_rgba(v).is_some(),
        _ => false,
    }
}

/// Whether changing param `key` would change the node's input pins (only
/// nodes with dynamic inputs can): tried with every dropdown option, else
/// a nudged number or the flipped boolean.
fn structural(cat: &Catalogue, spec: &NodeSpec, n: &PatchNode, key: &str) -> bool {
    if !spec.has_dynamic_inputs {
        return false;
    }
    let names = |params: &crate::spec::Params| -> Vec<String> {
        cat.resolved_inputs(spec, params)
            .into_iter()
            .map(|p| p.name)
            .collect()
    };
    let base = names(&n.params);
    let current = n.params.get(key).or_else(|| spec.default_params.get(key));
    let mut alts: Vec<Json> = spec
        .param_options
        .get(key)
        .map(|o| o.iter().map(|s| json!(s)).collect())
        .unwrap_or_default();
    match current {
        Some(Json::Number(x)) => {
            let x = x.as_f64().unwrap_or(0.0);
            alts.extend([json!(x + 1.0), json!(x - 1.0)]);
        }
        Some(Json::Bool(b)) => alts.push(json!(!b)),
        _ => {}
    }
    alts.into_iter().any(|alt| {
        let mut p = n.params.clone();
        p.insert(key.to_owned(), alt);
        names(&p) != base
    })
}

/// A pin's declared default as `.flora` JSON.
fn default_json(p: &PortSpec) -> Option<Json> {
    Some(match p.default.as_ref()? {
        Value::Num(x) => json!(x),
        Value::Bool(b) => json!(b),
        Value::Str(s) => json!(s.as_ref()),
        Value::Vec2(v) => json!({ "x": v.x, "y": v.y }),
        Value::Color(c) => json!({ "r": c.r, "g": c.g, "b": c.b, "a": c.a }),
        _ => return None,
    })
}

/// Snapshot the patch's current values (and seed) as a preset.
pub fn capture(cat: &Catalogue, patch: &PatchFile, name: impl Into<String>) -> Preset {
    let mut values = std::collections::BTreeMap::<String, PresetValues>::new();
    for f in fields(cat, patch) {
        let entry = values.entry(f.node).or_default();
        match f.key {
            Key::Param(k) => entry.params.insert(k, f.value),
            Key::Port(p) => entry.ports.insert(p, f.value),
        };
    }
    Preset {
        name: name.into(),
        seed: patch.seed,
        values,
    }
}

/// The value `p` holds for a field, if any.
pub fn stored<'a>(p: &'a Preset, node: &str, key: &Key) -> Option<&'a Json> {
    let v = p.values.get(node)?;
    match key {
        Key::Param(k) => v.params.get(k),
        Key::Port(n) => v.ports.get(n),
    }
}

/// The values (and seed) halfway-or-so between two presets: `t` = 0 is
/// `a`, 1 is `b`. Only fields the patch still has are returned; a field
/// one preset lacks keeps its current value on that side. Recalling a
/// preset is `morph(.., p, p, 0.0)`.
pub fn morph(
    cat: &Catalogue,
    patch: &PatchFile,
    a: &Preset,
    b: &Preset,
    t: f64,
) -> (Vec<Field>, u32) {
    let fields = fields(cat, patch)
        .into_iter()
        .filter_map(|f| {
            let va = stored(a, &f.node, &f.key);
            let vb = stored(b, &f.node, &f.key);
            if va.is_none() && vb.is_none() {
                return None;
            }
            let va = va.unwrap_or(&f.value);
            let vb = vb.unwrap_or(&f.value);
            Some(Field {
                value: lerp(va, vb, t, f.integer),
                ..f
            })
        })
        .collect();
    let seed = if t < 0.5 { a.seed } else { b.seed };
    (fields, seed)
}

/// Interpolate two preset values. Numbers, vec2s and colours (objects or
/// `#rrggbb[aa]` strings) blend; integer pins round; anything else, or a
/// mismatched pair, snaps at `t = 0.5`.
pub fn lerp(a: &Json, b: &Json, t: f64, integer: bool) -> Json {
    if t <= 0.0 {
        return a.clone();
    }
    if t >= 1.0 {
        return b.clone();
    }
    let mix = |x: f64, y: f64| x + (y - x) * t;
    if let (Json::Number(x), Json::Number(y)) = (a, b) {
        let (x, y) = (x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0));
        let v = mix(x, y);
        return if integer {
            json!(v.round() as i64)
        } else {
            json!(v)
        };
    }
    if let (Some(p), Some(q)) = (as_vec2(a), as_vec2(b)) {
        return json!({ "x": mix(p.0, q.0), "y": mix(p.1, q.1) });
    }
    if let (Some(p), Some(q)) = (as_rgba(a), as_rgba(b)) {
        return json!({
            "r": mix(p[0], q[0]),
            "g": mix(p[1], q[1]),
            "b": mix(p[2], q[2]),
            "a": mix(p[3], q[3]),
        });
    }
    if let (Json::String(x), Json::String(y)) = (a, b) {
        if let (Some(p), Some(q)) = (parse_hex(x), parse_hex(y)) {
            let c: [f64; 4] = std::array::from_fn(|i| mix(p[i], q[i]));
            return json!(hex(c, x.len() > 7 || y.len() > 7));
        }
    }
    if t < 0.5 {
        a.clone()
    } else {
        b.clone()
    }
}

fn as_vec2(v: &Json) -> Option<(f64, f64)> {
    let o = v.as_object()?;
    if o.len() != 2 {
        return None;
    }
    Some((o.get("x")?.as_f64()?, o.get("y")?.as_f64()?))
}

fn as_rgba(v: &Json) -> Option<[f64; 4]> {
    let o = v.as_object()?;
    let c = |k: &str| o.get(k).and_then(Json::as_f64);
    Some([c("r")?, c("g")?, c("b")?, c("a").unwrap_or(1.0)])
}

/// `#rrggbb` / `#rrggbbaa` → 0..1 channels.
fn parse_hex(s: &str) -> Option<[f64; 4]> {
    let h = s.strip_prefix('#')?;
    if !(h.len() == 6 || h.len() == 8) || !h.is_ascii() {
        return None;
    }
    let ch = |i: usize| {
        u8::from_str_radix(&h[i..i + 2], 16)
            .ok()
            .map(|v| f64::from(v) / 255.0)
    };
    Some([
        ch(0)?,
        ch(2)?,
        ch(4)?,
        if h.len() == 8 { ch(6)? } else { 1.0 },
    ])
}

fn hex(c: [f64; 4], alpha: bool) -> String {
    let b = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut s = format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]));
    if alpha {
        s.push_str(&format!("{:02x}", b(c[3])));
    }
    s
}

/// Write fields into the patch. Returns whether anything changed.
pub fn apply(patch: &mut PatchFile, fields: &[Field]) -> bool {
    let mut changed = false;
    for f in fields {
        let Some(n) = patch.nodes.iter_mut().find(|n| n.id == f.node) else {
            continue;
        };
        let slot = match &f.key {
            Key::Param(k) => {
                // Saved files often mirror a param-backed pin in
                // `portValues` too; keep the copy in step (as the editor does).
                if let Some(m) = n.port_values.as_mut().and_then(|pv| pv.get_mut(k)) {
                    *m = f.value.clone();
                }
                n.params.entry(k.clone())
            }
            Key::Port(p) => n
                .port_values
                .get_or_insert_with(Default::default)
                .entry(p.clone()),
        };
        match slot {
            serde_json::map::Entry::Occupied(mut o) => {
                if o.get() != &f.value {
                    o.insert(f.value.clone());
                    changed = true;
                }
            }
            serde_json::map::Entry::Vacant(e) => {
                e.insert(f.value.clone());
                changed = true;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(text: &str) -> PatchFile {
        crate::load_patch(text).unwrap()
    }

    const PATCH: &str = r##"{"kind":"floret.patch","version":2,"nodes":[
        {"id":"slider","defType":"io/slider","position":{"x":0,"y":0},
         "params":{"value":0.25,"min":0,"max":1}},
        {"id":"circle","defType":"draw/ellipse","position":{"x":200,"y":0},"params":{},
         "portValues":{"w":40,"pos":{"x":10,"y":-10}}},
        {"id":"add","defType":"math/add","position":{"x":0,"y":200},
         "params":{"valueType":"number","slotCount":2}},
        {"id":"rnd","defType":"random/uniform","position":{"x":0,"y":300},"params":{}}],
        "edges":[{"id":"e","source":"slider","target":"circle",
         "sourceHandle":"out-value","targetHandle":"in-w"}]}"##;

    fn keys(p: &PatchFile, node: &str) -> Vec<Key> {
        fields(Catalogue::builtin(), p)
            .into_iter()
            .filter(|f| f.node == node)
            .map(|f| f.key)
            .collect()
    }

    #[test]
    fn fields_are_the_hand_tuned_values() {
        let p = patch(PATCH);
        let slider = keys(&p, "slider");
        assert!(slider.contains(&Key::Param("value".into())));
        // Slider ranges are setup, not values.
        assert!(!slider.contains(&Key::Param("min".into())));
        let circle = keys(&p, "circle");
        assert!(circle.contains(&Key::Port("pos".into())));
        assert!(circle.contains(&Key::Port("h".into())));
        // Wired pins are left out.
        assert!(!circle.contains(&Key::Port("w".into())));
        // valueType / slotCount reshape the node: never in a preset.
        let add = keys(&p, "add");
        assert!(!add.contains(&Key::Param("valueType".into())));
        assert!(!add.contains(&Key::Param("slotCount".into())));
        // Unset pins are captured at their default.
        let rnd = fields(Catalogue::builtin(), &p)
            .into_iter()
            .find(|f| f.node == "rnd" && f.key == Key::Port("seed".into()))
            .expect("seed pin");
        assert!(rnd.value.is_number());
    }

    #[test]
    fn mode_that_swaps_pins_is_structural() {
        let cat = Catalogue::builtin();
        let spec = cat.get("color/color").unwrap();
        let mut n = crate::patch::PatchNode {
            id: "c".into(),
            def_type: "color/color".into(),
            position: Default::default(),
            params: spec.default_params.clone(),
            parent_id: None,
            port_values: None,
            ports: None,
            width: None,
            height: None,
            bypassed: false,
            extra: Default::default(),
        };
        assert!(structural(cat, spec, &n, "mode"));
        n.def_type = "draw/ellipse".into();
        assert!(!structural(
            cat,
            cat.get("draw/ellipse").unwrap(),
            &n,
            "anchor"
        ));
    }

    #[test]
    fn capture_recall_round_trip() {
        let cat = Catalogue::builtin();
        let mut p = patch(PATCH);
        p.seed = 77;
        let a = capture(cat, &p, "A");
        assert_eq!(a.seed, 77);
        assert_eq!(
            stored(&a, "slider", &Key::Param("value".into())),
            Some(&json!(0.25))
        );
        // Change things, then recall.
        p.nodes[0].params.insert("value".into(), json!(0.9));
        p.nodes[1]
            .port_values
            .as_mut()
            .unwrap()
            .insert("pos".into(), json!({"x": 0, "y": 0}));
        p.seed = 5;
        let (writes, seed) = morph(cat, &p, &a, &a, 0.0);
        assert!(apply(&mut p, &writes));
        p.seed = seed;
        assert_eq!(p.nodes[0].params["value"], json!(0.25));
        assert_eq!(
            p.nodes[1].port_values.as_ref().unwrap()["pos"],
            json!({"x": 10, "y": -10})
        );
        assert_eq!(p.seed, 77);
        // Recalling again changes nothing.
        let (writes, _) = morph(cat, &p, &a, &a, 0.0);
        assert!(!apply(&mut p, &writes));
    }

    #[test]
    fn morph_interpolates_and_snaps() {
        let cat = Catalogue::builtin();
        let mut p = patch(PATCH);
        p.seed = 1;
        let a = capture(cat, &p, "A");
        p.nodes[0].params.insert("value".into(), json!(0.75));
        p.nodes[1]
            .port_values
            .as_mut()
            .unwrap()
            .insert("pos".into(), json!({"x": 30, "y": 10}));
        p.seed = 2;
        let b = capture(cat, &p, "B");
        let at = |t: f64| {
            let (w, seed) = morph(cat, &p, &a, &b, t);
            let get = |node: &str, key: Key| {
                w.iter()
                    .find(|f| f.node == node && f.key == key)
                    .unwrap()
                    .value
                    .clone()
            };
            (
                get("slider", Key::Param("value".into())),
                get("circle", Key::Port("pos".into())),
                seed,
            )
        };
        assert_eq!(at(0.0), (json!(0.25), json!({"x": 10, "y": -10}), 1));
        assert_eq!(at(0.5), (json!(0.5), json!({"x": 20.0, "y": 0.0}), 2));
        assert_eq!(at(0.25).2, 1);
        assert_eq!(at(1.0), (json!(0.75), json!({"x": 30, "y": 10}), 2));
    }

    #[test]
    fn lerp_rules() {
        assert_eq!(lerp(&json!(0), &json!(10), 0.25, false), json!(2.5));
        assert_eq!(lerp(&json!(0), &json!(10), 0.26, true), json!(3));
        let c = lerp(
            &json!({"r": 0, "g": 0, "b": 0, "a": 1}),
            &json!({"r": 1, "g": 0.5, "b": 0, "a": 0}),
            0.5,
            false,
        );
        assert_eq!(c, json!({"r": 0.5, "g": 0.25, "b": 0.0, "a": 0.5}));
        assert_eq!(
            lerp(&json!("#000000"), &json!("#ff8000"), 0.5, false),
            json!("#804000")
        );
        // Discrete values snap at the midpoint.
        assert_eq!(
            lerp(&json!("sine"), &json!("saw"), 0.49, false),
            json!("sine")
        );
        assert_eq!(
            lerp(&json!("sine"), &json!("saw"), 0.5, false),
            json!("saw")
        );
        assert_eq!(lerp(&json!(true), &json!(false), 0.7, false), json!(false));
        // Mismatched kinds snap too.
        assert_eq!(lerp(&json!(1), &json!("x"), 0.2, false), json!(1));
    }
}
