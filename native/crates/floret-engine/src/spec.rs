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
    pub label: String,
    pub description: Option<String>,
    pub inputs: Vec<PortSpec>,
    pub outputs: Vec<PortSpec>,
    pub eval: EvalKind,
    pub has_dynamic_inputs: bool,
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

/// All node specs, keyed by type, in catalogue order.
pub struct Catalogue {
    specs: IndexMap<String, NodeSpec>,
    dynamic: HashMap<&'static str, DynamicInputsFn>,
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
    label: String,
    description: Option<String>,
    inputs: Vec<RawPort>,
    outputs: Vec<RawPort>,
    eval: EvalKind,
    #[serde(default)]
    dynamic_inputs: bool,
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
            category: self.category,
            label: self.label,
            description: self.description,
            inputs: ports(self.inputs)?,
            outputs: ports(self.outputs)?,
            eval: self.eval,
            has_dynamic_inputs: self.dynamic_inputs,
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
    }

    #[test]
    #[ignore = "enable once all 17 dynamic-input node types are ported"]
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
