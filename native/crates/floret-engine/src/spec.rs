//! Node specifications: ports, defaults and flags for every node type.
//!
//! The static part of every spec is data (`nodes.json`, seeded from the
//! TypeScript registry). Behaviour that depends on a node's params — the 17
//! node types whose input pins change with a mode or slot count — is code,
//! registered per type in `nodes::dynamic_inputs`.

use crate::value::Value;
use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

pub type Params = serde_json::Map<String, serde_json::Value>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortType {
    Number,
    Boolean,
    String,
    Vec2,
    Color,
    Mat3,
    Layer,
    Style,
    Audio,
    Image,
    World,
    Force,
    Collider,
    Particle,
    Any,
}

impl PortType {
    pub fn parse(s: &str) -> Option<PortType> {
        serde_json::from_value(serde_json::Value::String(s.to_owned())).ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PortKind {
    #[default]
    Single,
    Spread,
    /// Input only: N wires gathered into one spread.
    Multi,
}

#[derive(Clone, Debug)]
pub struct PortSpec {
    pub name: String,
    pub label: Option<String>,
    pub ty: PortType,
    pub kind: PortKind,
    /// `None` = no default declared; `Some(Value::Null)` = explicit null.
    pub default: Option<Value>,
    /// Inline-editor param bound to this pin (read live when unwired).
    pub param: Option<String>,
    pub integer: bool,
    /// Sampled once, never re-gathered per tick.
    pub constant: bool,
}

impl PortSpec {
    pub fn new(name: impl Into<String>, ty: PortType) -> PortSpec {
        PortSpec {
            name: name.into(),
            label: None,
            ty,
            kind: PortKind::Single,
            default: None,
            param: None,
            integer: false,
            constant: false,
        }
    }

    pub fn kind(mut self, kind: PortKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn default(mut self, v: Value) -> Self {
        self.default = Some(v);
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvalKind {
    /// Scalar compute: single inputs unwrapped, called once per tick.
    Scalar,
    /// Raw compute: whole spreads in, whole spreads out, with params.
    Raw,
    /// Source node: reads the frame / params, no inputs gathered.
    Context,
    /// No compute (regions, splicers, state, annotations).
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RegionKind {
    Foreach,
    If,
    Cache,
    Subpatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RegionRole {
    SplicerIn,
    SplicerOut,
    AccumulatorRead,
    AccumulatorWrite,
    LoopVar,
}

#[derive(Clone, Debug)]
pub struct NodeSpec {
    pub ty: String,
    pub category: String,
    /// The node browser group (one of [`GROUPS`]); the category when the
    /// catalogue doesn't give one.
    pub group: String,
    pub label: String,
    pub description: Option<String>,
    pub inputs: Vec<PortSpec>,
    pub outputs: Vec<PortSpec>,
    pub eval: EvalKind,
    pub has_dynamic_inputs: bool,
    /// Output ports depend on params / upstream types (editor display only;
    /// compile indexes the static outputs).
    pub has_dynamic_outputs: bool,
    pub default_params: Params,
    pub param_options: HashMap<String, Vec<String>>,
    pub stateful: bool,
    pub effectful: bool,
    pub stateful_next_port: Option<usize>,
    pub region_kind: Option<RegionKind>,
    pub region_role: Option<RegionRole>,
    pub hidden: bool,
    pub compact: bool,
    pub adaptive_type: bool,
}

pub type DynamicInputsFn = fn(&Params) -> Vec<PortSpec>;

/// Whether a node's pins adapt to `valueType` (number / vec2 / colour) for
/// these params. Registered for node types that only adapt in some modes.
pub type AdaptiveFn = fn(&Params) -> bool;

/// A node browser group: what the nodes in it are *for*, in roughly the
/// order a patch is built (see [`GROUPS`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Group {
    pub name: &'static str,
    /// One line for the browser's tooltip.
    pub blurb: &'static str,
}

/// The node browser groups, in display order. Every node in `nodes.json`
/// names one (`group`); the type ids and categories are unaffected.
pub const GROUPS: &[Group] = &[
    Group {
        name: "Source",
        blurb: "Values, controls and files: numbers, sliders, toggles, images, SVG, video, CSV.",
    },
    Group {
        name: "Draw",
        blurb: "Shapes and images as layers, and the Renderer that shows them.",
    },
    Group {
        name: "Text",
        blurb: "Type: text, glyphs, text on a path, outlines, fonts and strings.",
    },
    Group {
        name: "Color & Style",
        blurb: "Colours, gradients and palettes; fill, stroke and other styles.",
    },
    Group {
        name: "Shape Ops",
        blurb: "Transform, group, blend and mask layers; boolean and path operations.",
    },
    Group {
        name: "Patterns",
        blurb: "Point layouts and sequences: grids, rings, spirals, scatter, Voronoi, L-systems, cellular automata.",
    },
    Group {
        name: "Noise & Random",
        blurb: "Noise fields, random numbers and shuffles.",
    },
    Group {
        name: "Animate",
        blurb: "Time, oscillators, easing, smoothing, springs, triggers and counters.",
    },
    Group {
        name: "Image",
        blurb: "Rasterize, crop, resize, sample and edit pixels.",
    },
    Group {
        name: "Effects",
        blurb: "Post effects and shaders, filters, blur, pixelate, pixel sort.",
    },
    Group {
        name: "Particles & Physics",
        blurb: "Emitters, simulations, forces and colliders.",
    },
    Group {
        name: "Input & Sound",
        blurb: "Mouse, keyboard, MIDI, camera and tracking; audio in, analysis and synthesis.",
    },
    Group {
        name: "Math",
        blurb: "Math, vector math, comparisons, logic, mapping, statistics and conversions.",
    },
    Group {
        name: "Lists",
        blurb: "Build, reorder, slice and combine spreads.",
    },
    Group {
        name: "Structure",
        blurb: "ForEach, If and Cache regions, state across frames, subpatches, comments and probes.",
    },
];

/// All node specs, keyed by type, in catalogue order.
pub struct Catalogue {
    specs: IndexMap<String, NodeSpec>,
    dynamic: HashMap<&'static str, DynamicInputsFn>,
    dynamic_outputs: HashMap<&'static str, DynamicInputsFn>,
    adaptive: HashMap<&'static str, AdaptiveFn>,
}

impl Catalogue {
    /// The built-in catalogue (parsed once).
    pub fn builtin() -> &'static Catalogue {
        static CAT: OnceLock<Catalogue> = OnceLock::new();
        CAT.get_or_init(|| {
            let mut cat = Catalogue::from_json(include_str!("nodes.json"))
                .expect("built-in nodes.json is valid");
            for (ty, f) in crate::nodes::dynamic_inputs() {
                cat.dynamic.insert(ty, f);
            }
            for (ty, f) in crate::nodes::dynamic_outputs() {
                cat.dynamic_outputs.insert(ty, f);
            }
            for (ty, f) in crate::nodes::adaptive_when() {
                cat.adaptive.insert(ty, f);
            }
            cat
        })
    }

    pub fn from_json(text: &str) -> Result<Catalogue, String> {
        let raw: RawCatalogue = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut specs = IndexMap::new();
        for n in raw.nodes {
            let spec = n.into_spec()?;
            specs.insert(spec.ty.clone(), spec);
        }
        Ok(Catalogue {
            specs,
            dynamic: HashMap::new(),
            dynamic_outputs: HashMap::new(),
            adaptive: HashMap::new(),
        })
    }

    pub fn get(&self, ty: &str) -> Option<&NodeSpec> {
        self.specs.get(ty)
    }

    pub fn iter(&self) -> impl Iterator<Item = &NodeSpec> {
        self.specs.values()
    }

    pub fn len(&self) -> usize {
        self.specs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// The node's active input ports for these params (`getResolvedInputs`).
    pub fn resolved_inputs(&self, spec: &NodeSpec, params: &Params) -> Vec<PortSpec> {
        match self.dynamic.get(spec.ty.as_str()) {
            Some(f) => f(params),
            None => spec.inputs.clone(),
        }
    }

    /// The node's output ports for these params. Names never change (the
    /// compiler indexes the static outputs); types may: a registered
    /// dynamic-outputs function's, or an adaptive node's latched
    /// `valueType`.
    pub fn resolved_outputs(&self, spec: &NodeSpec, params: &Params) -> Vec<PortSpec> {
        let mut outs = match self.dynamic_outputs.get(spec.ty.as_str()) {
            Some(f) => f(params),
            None => spec.outputs.clone(),
        };
        if self.is_adaptive(spec, params) {
            let ty = crate::nodes::math::adaptive::port_type_for_value_type(params);
            for p in &mut outs {
                p.ty = ty;
            }
        }
        outs
    }

    /// Whether the node's pins currently follow the `valueType` latch: an
    /// adaptive type, in a mode that adapts (a Math node set to `add` does,
    /// one set to `sin` doesn't).
    pub fn is_adaptive(&self, spec: &NodeSpec, params: &Params) -> bool {
        spec.adaptive_type
            && self
                .adaptive
                .get(spec.ty.as_str())
                .is_none_or(|f| f(params))
    }

    /// Whether a dynamic-input function is implemented for this type.
    pub fn has_dynamic_impl(&self, ty: &str) -> bool {
        self.dynamic.contains_key(ty)
    }
}

// ─── JSON shape ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RawCatalogue {
    nodes: Vec<RawNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPort {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    kind: PortKind,
    label: Option<String>,
    /// Missing → `None`; an explicit `null` → `Some(Null)` (TS
    /// `defaultForPort` tells them apart; plain `Option` would merge them).
    #[serde(default, deserialize_with = "explicit_default")]
    default: Option<serde_json::Value>,
    param: Option<String>,
    #[serde(default)]
    integer: bool,
    #[serde(default)]
    constant: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawNode {
    #[serde(rename = "type")]
    ty: String,
    category: String,
    group: Option<String>,
    label: String,
    description: Option<String>,
    inputs: Vec<RawPort>,
    outputs: Vec<RawPort>,
    eval: EvalKind,
    #[serde(default)]
    dynamic_inputs: bool,
    #[serde(default)]
    dynamic_outputs: bool,
    default_params: Option<Params>,
    param_options: Option<HashMap<String, Vec<String>>>,
    #[serde(default)]
    stateful: bool,
    #[serde(default)]
    effectful: bool,
    stateful_next_port: Option<usize>,
    region_kind: Option<RegionKind>,
    region_role: Option<RegionRole>,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    compact: bool,
    #[serde(default)]
    adaptive_type: bool,
}

fn explicit_default<'de, D>(d: D) -> Result<Option<serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde_json::Value::deserialize(d).map(Some)
}

impl RawPort {
    fn into_spec(self) -> Result<PortSpec, String> {
        let ty =
            PortType::parse(&self.ty).ok_or_else(|| format!("unknown port type {}", self.ty))?;
        Ok(PortSpec {
            name: self.name,
            label: self.label,
            ty,
            kind: self.kind,
            default: self.default.as_ref().map(Value::from_json),
            param: self.param,
            integer: self.integer,
            constant: self.constant,
        })
    }
}

impl RawNode {
    fn into_spec(self) -> Result<NodeSpec, String> {
        let ports = |v: Vec<RawPort>| {
            v.into_iter()
                .map(RawPort::into_spec)
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(NodeSpec {
            ty: self.ty,
            group: self.group.unwrap_or_else(|| self.category.clone()),
            category: self.category,
            label: self.label,
            description: self.description,
            inputs: ports(self.inputs)?,
            outputs: ports(self.outputs)?,
            eval: self.eval,
            has_dynamic_inputs: self.dynamic_inputs,
            has_dynamic_outputs: self.dynamic_outputs,
            default_params: self.default_params.unwrap_or_default(),
            param_options: self.param_options.unwrap_or_default(),
            stateful: self.stateful,
            effectful: self.effectful,
            stateful_next_port: self.stateful_next_port,
            region_kind: self.region_kind,
            region_role: self.region_role,
            hidden: self.hidden,
            compact: self.compact,
            adaptive_type: self.adaptive_type,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalogue_loads() {
        let cat = Catalogue::builtin();
        assert!(cat.len() >= 300, "got {}", cat.len());
        let r = cat.get("render/renderer").unwrap();
        assert_eq!(r.inputs[0].kind, PortKind::Spread);
        assert_eq!(r.inputs[2].param.as_deref(), Some("width"));
        // An explicit `default: null` is kept (not the type's fallback).
        let fill = cat.get("style/fill").unwrap();
        assert!(matches!(fill.inputs[0].default, Some(Value::Null)));
        assert!(fill.inputs[1].default.is_none());
        let w = cat.get("physics/world").unwrap();
        assert!(matches!(w.inputs[3].default, Some(Value::Null)));
        assert!(w.inputs[4].default.is_none());
    }

    #[test]
    fn every_node_has_a_browser_group_and_no_group_is_empty() {
        let cat = Catalogue::builtin();
        let names: Vec<&str> = GROUPS.iter().map(|g| g.name).collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "duplicate group");
        // Hidden legacy nodes too: old patches colour them by group.
        let stray: Vec<_> = cat
            .iter()
            .filter(|s| !names.contains(&s.group.as_str()))
            .map(|s| (s.ty.clone(), s.group.clone()))
            .collect();
        assert!(stray.is_empty(), "nodes without a known group: {stray:?}");
        for g in GROUPS {
            let n = cat
                .iter()
                .filter(|s| !s.hidden && s.group == g.name)
                .count();
            assert!(
                (5..=40).contains(&n),
                "group {} has {n} browsable nodes",
                g.name
            );
        }
        assert_eq!(cat.get("math/math").unwrap().group, "Math");
        assert_eq!(cat.get("render/renderer").unwrap().group, "Draw");
        // A catalogue without groups falls back to the category.
        let c = Catalogue::from_json(
            r#"{"nodes":[{"type":"a/b","category":"Thing","label":"B","inputs":[],"outputs":[],"eval":"raw"}]}"#,
        )
        .unwrap();
        assert_eq!(c.get("a/b").unwrap().group, "Thing");
    }

    #[test]
    fn every_dynamic_node_has_an_implementation() {
        let cat = Catalogue::builtin();
        let missing: Vec<_> = cat
            .iter()
            .filter(|s| s.has_dynamic_inputs && !cat.has_dynamic_impl(&s.ty))
            .map(|s| s.ty.clone())
            .collect();
        assert!(missing.is_empty(), "dynamic inputs not ported: {missing:?}");
    }
}
