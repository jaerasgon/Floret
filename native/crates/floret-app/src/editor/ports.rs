//! Port lookup for editing: a handle on a patch node resolved to its
//! `{type, kind}`, honouring dynamic inputs, region frame ports, loop
//! variables and the adaptive-math `valueType` latch.

use floret_engine::nodes::math::adaptive::port_type_for_value_type;
use floret_engine::patch::{parse_handle, Handle, PatchFile, PatchNode, PortDirection};
use floret_engine::spec::{Catalogue, NodeSpec, PortKind, PortSpec, PortType};

/// Which side of a node a pin sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    In,
    Out,
}

impl Side {
    pub fn prefix(self) -> &'static str {
        match self {
            Side::In => "in-",
            Side::Out => "out-",
        }
    }

    /// The handle id for a port name on this side.
    pub fn handle(self, name: &str) -> String {
        format!("{}{name}", self.prefix())
    }
}

/// Split a handle id into side and port name.
pub fn split_handle(h: Option<&str>) -> Option<(Side, &str)> {
    match parse_handle(h)? {
        Handle::In(n) => Some((Side::In, n)),
        Handle::Out(n) => Some((Side::Out, n)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedPort {
    pub ty: PortType,
    pub kind: PortKind,
}

pub fn find_node<'a>(patch: &'a PatchFile, id: &str) -> Option<&'a PatchNode> {
    patch.nodes.iter().find(|n| n.id == id)
}

pub fn find_node_mut<'a>(patch: &'a mut PatchFile, id: &str) -> Option<&'a mut PatchNode> {
    patch.nodes.iter_mut().find(|n| n.id == id)
}

/// `parseLoopVarHandle`: the built-in loop-variable pins of foreach/cache.
pub fn is_loop_var(name: &str) -> bool {
    matches!(
        name.strip_prefix("__loopvar_"),
        Some("index" | "count" | "fraction")
    )
}

/// Output ports for display and validation. Adaptive nodes report their
/// latched `valueType` on `result`; other dynamic outputs fall back to the
/// static spec (the engine has no `dynamicOutputs` hook yet).
pub fn resolved_outputs(spec: &NodeSpec, params: &floret_engine::spec::Params) -> Vec<PortSpec> {
    let mut outs = spec.outputs.clone();
    if spec.adaptive_type {
        let ty = port_type_for_value_type(params);
        for p in &mut outs {
            p.ty = ty;
        }
    }
    outs
}

/// `resolvePort` (connection-rules.ts).
pub fn resolve_port(
    patch: &PatchFile,
    cat: &Catalogue,
    node_id: &str,
    side: Side,
    name: &str,
) -> Option<ResolvedPort> {
    let n = find_node(patch, node_id)?;
    let spec = cat.get(&n.def_type)?;
    if let Some(kind) = spec.region_kind {
        use floret_engine::spec::RegionKind;
        if matches!(kind, RegionKind::Foreach | RegionKind::Cache)
            && side == Side::Out
            && is_loop_var(name)
        {
            return Some(ResolvedPort {
                ty: PortType::Number,
                kind: PortKind::Single,
            });
        }
        if let Some(p) = n
            .ports
            .as_ref()
            .and_then(|ps| ps.iter().find(|p| p.id == name))
        {
            // Input frame ports: outer `in-` gathers a spread, inner `out-`
            // emits one element. Output frame ports are the mirror image.
            let kind = match (p.direction, side) {
                (PortDirection::Input, Side::In) | (PortDirection::Output, Side::Out) => {
                    PortKind::Spread
                }
                _ => PortKind::Single,
            };
            return Some(ResolvedPort {
                ty: PortType::parse(&p.ty).unwrap_or(PortType::Any),
                kind,
            });
        }
        // Fall through to static inputs (e.g. If.condition).
    }
    let list = match side {
        Side::In => cat.resolved_inputs(spec, &n.params),
        Side::Out => resolved_outputs(spec, &n.params),
    };
    list.iter().find(|p| p.name == name).map(|p| ResolvedPort {
        ty: p.ty,
        kind: p.kind,
    })
}

/// Port type name as used by the theme and `.flora` region ports.
pub fn type_name(t: PortType) -> &'static str {
    match t {
        PortType::Number => "number",
        PortType::Boolean => "boolean",
        PortType::String => "string",
        PortType::Vec2 => "vec2",
        PortType::Color => "color",
        PortType::Mat3 => "mat3",
        PortType::Layer => "layer",
        PortType::Style => "style",
        PortType::Audio => "audio",
        PortType::Image => "image",
        PortType::World => "world",
        PortType::Force => "force",
        PortType::Collider => "collider",
        PortType::Particle => "particle",
        PortType::Any => "any",
    }
}
