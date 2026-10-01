//! Node search filtering (`NodeSearch.tsx`): token match over label /
//! type / category / description, restricted to nodes that can take the
//! dragged wire (or sit inside a wire being split).

use super::connect::are_ports_compatible;
use super::ports::{resolved_outputs, Side};
use floret_engine::spec::{Catalogue, NodeSpec, PortSpec, PortType};

pub const MAX_RESULTS: usize = 40;

/// What the search must be compatible with.
#[derive(Clone, Debug, PartialEq)]
pub enum Fit {
    Any,
    /// A wire dragged from a pin on `side` carrying `ty`; the new node
    /// needs a pin on the opposite side.
    Pin { side: Side, ty: PortType },
    /// Insert into a wire: needs an input taking `src` and an output
    /// feeding `dst`.
    Insert { src: PortType, dst: PortType },
}

fn inputs(cat: &Catalogue, s: &NodeSpec) -> Vec<PortSpec> {
    cat.resolved_inputs(s, &s.default_params)
}

/// The pin on a fresh `spec` node that a `side`/`ty` wire would land on.
pub fn first_compatible(cat: &Catalogue, s: &NodeSpec, side: Side, ty: PortType) -> Option<String> {
    let ports = match side {
        Side::Out => inputs(cat, s),
        Side::In => resolved_outputs(s, &s.default_params),
    };
    ports
        .into_iter()
        .find(|p| match side {
            Side::Out => are_ports_compatible(ty, p.ty),
            Side::In => are_ports_compatible(p.ty, ty),
        })
        .map(|p| p.name)
}

/// For an insertion: `(input name, output name)` on a fresh `spec` node.
pub fn insertion_ports(
    cat: &Catalogue,
    s: &NodeSpec,
    src: PortType,
    dst: PortType,
) -> Option<(String, String)> {
    Some((
        first_compatible(cat, s, Side::Out, src)?,
        first_compatible(cat, s, Side::In, dst)?,
    ))
}

pub fn fits(cat: &Catalogue, s: &NodeSpec, fit: &Fit) -> bool {
    match fit {
        Fit::Any => true,
        Fit::Pin { side, ty } => first_compatible(cat, s, *side, *ty).is_some(),
        Fit::Insert { src, dst } => insertion_ports(cat, s, *src, *dst).is_some(),
    }
}

pub fn search<'a>(cat: &'a Catalogue, query: &str, fit: &Fit) -> Vec<&'a NodeSpec> {
    let q = query.trim().to_lowercase();
    let tokens: Vec<&str> = q.split_whitespace().collect();
    let mut matches: Vec<&NodeSpec> = cat
        .iter()
        .filter(|s| !s.hidden && fits(cat, s, fit))
        .filter(|s| {
            let hay = format!(
                "{} {} {} {}",
                s.label,
                s.ty,
                s.category,
                s.description.as_deref().unwrap_or("")
            )
            .to_lowercase();
            tokens.iter().all(|t| hay.contains(t))
        })
        .collect();
    matches.sort_by(|a, b| {
        let prefix = |s: &NodeSpec| !(q.is_empty() || s.label.to_lowercase().starts_with(&q));
        prefix(a)
            .cmp(&prefix(b))
            .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
            .then_with(|| a.label.cmp(&b.label))
    });
    matches.truncate(MAX_RESULTS);
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_by_wire_type() {
        let cat = Catalogue::builtin();
        let layers = search(
            cat,
            "",
            &Fit::Pin {
                side: Side::Out,
                ty: PortType::Layer,
            },
        );
        assert!(!layers.is_empty());
        for s in &layers {
            assert!(first_compatible(cat, s, Side::Out, PortType::Layer).is_some());
        }
        assert!(layers.iter().all(|s| s.ty != "time/seconds"));
        // Multi-token match + prefix-first ordering.
        let r = search(cat, "add", &Fit::Any);
        assert_eq!(r[0].label.to_lowercase().starts_with("add"), true);
        assert!(search(cat, "zzzz-nothing", &Fit::Any).is_empty());
    }

    #[test]
    fn insertion_needs_both_sides() {
        let cat = Catalogue::builtin();
        let r = search(
            cat,
            "",
            &Fit::Insert {
                src: PortType::Number,
                dst: PortType::Vec2,
            },
        );
        for s in &r {
            assert!(insertion_ports(cat, s, PortType::Number, PortType::Vec2).is_some());
        }
        assert!(r.iter().any(|s| s.ty == "vec2/make"));
    }
}
