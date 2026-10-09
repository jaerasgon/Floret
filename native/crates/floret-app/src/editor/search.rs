//! Node search filtering (`NodeSearch.tsx`): token match over label /
//! type / category / group / description, restricted to nodes that can
//! take the dragged wire (or sit inside a wire being split).
//!
//! Besides node types, a search lists the ops of the op-family nodes as
//! entries of their own — typing "mul" offers *Multiply*, which places a
//! Math node set to multiply — so the single-op nodes those families
//! replaced are still one search away, under their old names and type ids.

use super::connect::are_ports_compatible;
use super::ports::{resolved_outputs, Side};
use floret_engine::nodes::family::{self, Family, Op};
use floret_engine::spec::{Catalogue, NodeSpec, Params, PortSpec, PortType};

pub const MAX_RESULTS: usize = 40;

/// What the search must be compatible with.
#[derive(Clone, Debug, PartialEq)]
pub enum Fit {
    Any,
    /// A wire dragged from a pin on `side` carrying `ty`; the new node
    /// needs a pin on the opposite side.
    Pin {
        side: Side,
        ty: PortType,
    },
    /// Insert into a wire: needs an input taking `src` and an output
    /// feeding `dst`.
    Insert {
        src: PortType,
        dst: PortType,
    },
}

/// Separates a node type from an op in an entry key (`math/math#sin`).
const OP_SEP: char = '#';

/// A placeable entry key: the node type, and the op it's preset to.
pub fn split_key(key: &str) -> (&str, Option<&str>) {
    match key.split_once(OP_SEP) {
        Some((ty, op)) => (ty, Some(op)),
        None => (key, None),
    }
}

/// One thing the browser or search can place: a node type, or an op
/// family node preset to one of its ops.
#[derive(Clone, Copy, Debug)]
pub struct Entry<'a> {
    pub spec: &'a NodeSpec,
    pub op: Option<(&'static Family, &'static Op)>,
}

impl<'a> Entry<'a> {
    pub fn node(spec: &'a NodeSpec) -> Entry<'a> {
        Entry { spec, op: None }
    }

    /// The key a pick / drag / favourite carries (see [`split_key`]).
    pub fn key(&self) -> String {
        match self.op {
            Some((_, o)) => format!("{}{OP_SEP}{}", self.spec.ty, o.id),
            None => self.spec.ty.clone(),
        }
    }

    pub fn label(&self) -> &str {
        self.op.map_or(&self.spec.label, |(_, o)| o.label)
    }

    /// Secondary text naming the node an op entry places ("Math").
    pub fn owner(&self) -> Option<&str> {
        self.op.map(|_| self.spec.label.as_str())
    }

    /// The params a fresh node gets: the type's defaults, plus the op.
    pub fn params(&self) -> Params {
        let mut p = self.spec.default_params.clone();
        if let Some((f, o)) = self.op {
            p.insert(f.param.into(), o.id.into());
        }
        p
    }

    /// The superseded single-op node an op entry stands for.
    fn legacy<'c>(&self, cat: &'c Catalogue) -> Option<&'c NodeSpec> {
        self.op
            .and_then(|(_, o)| o.legacy())
            .and_then(|t| cat.get(t))
    }

    /// What it does: an op entry's own node's description.
    pub fn description<'c>(&self, cat: &'c Catalogue) -> Option<&'c str>
    where
        'a: 'c,
    {
        self.legacy(cat)
            .and_then(|s| s.description.as_deref())
            .or(self.spec.description.as_deref())
    }

    /// The names a query's tokens are matched against: label (also with
    /// its spaces closed up, the old CamelCase spelling: "Sine Spread" is
    /// still "SineSpread"), type id, category and group. An op entry is
    /// named by its op and the node it supersedes (label, type id,
    /// category), not by its family's, so "math" lists the Math node
    /// rather than all 28 of its ops. Case is kept: CamelCase marks word
    /// starts (see [`word_match`]).
    fn names(&self, cat: &Catalogue) -> String {
        let s = self.spec;
        let closed = |l: &str| l.replace(' ', "");
        match self.op {
            None => format!(
                "{} {} {} {} {}",
                s.label,
                closed(&s.label),
                s.ty,
                s.category,
                s.group
            ),
            Some((_, o)) => {
                let legacy = self.legacy(cat);
                format!(
                    "{} {} {} {} {} {}",
                    o.label,
                    o.id,
                    s.label,
                    legacy.map_or("", |l| l.ty.as_str()),
                    legacy.map_or("", |l| l.label.as_str()),
                    legacy.map_or("", |l| l.category.as_str()),
                )
            }
        }
    }

    /// Names plus (for a node type) its description.
    fn haystack(&self, cat: &Catalogue) -> String {
        match (self.op, &self.spec.description) {
            (None, Some(d)) => format!("{} {d}", self.names(cat)),
            _ => self.names(cat),
        }
    }
}

/// Whether lowercase `token` starts at a word start in `hay`: the start of
/// the text, after anything not a letter or digit ("math/mul", "to-string"),
/// at a CamelCase hump ("SineSpread", "CAGrid"), or where digits begin. So
/// "mul" finds Multiply but not accumulator, "spread" finds SineSpread.
pub fn word_match(hay: &str, token: &str) -> bool {
    if token.is_empty() {
        return true;
    }
    let chars: Vec<char> = hay.chars().collect();
    let lower: Vec<char> = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let want: Vec<char> = token.chars().collect();
    (0..chars.len()).any(|i| {
        let start = i == 0 || {
            let (prev, cur) = (chars[i - 1], chars[i]);
            let next = chars.get(i + 1).copied();
            !prev.is_alphanumeric()
                || (prev.is_lowercase() && cur.is_uppercase())
                || (prev.is_uppercase()
                    && cur.is_uppercase()
                    && next.is_some_and(char::is_lowercase))
                || (!prev.is_ascii_digit() && cur.is_ascii_digit())
        };
        start && lower[i..].starts_with(&want)
    })
}

/// The entry a key names (hidden node types too: favourites and drags of
/// them keep working).
pub fn entry<'a>(cat: &'a Catalogue, key: &str) -> Option<Entry<'a>> {
    let (ty, op) = split_key(key);
    let spec = cat.get(ty)?;
    let op = match op {
        None => None,
        Some(id) => {
            let f = family::family(ty)?;
            Some((f, f.op_by_id(id)?))
        }
    };
    Some(Entry { spec, op })
}

/// A starred key as a browsable entry: a star on a single-op node that an
/// op family replaced (`math/mul`) now stands for that op (Math set to
/// multiply); other hidden types drop out.
pub fn favorite<'a>(cat: &'a Catalogue, key: &str) -> Option<Entry<'a>> {
    let e = entry(cat, key)?;
    if !e.spec.hidden {
        return Some(e);
    }
    let (f, o) = family::superseded_by(&e.spec.ty)?;
    Some(Entry {
        spec: cat.get(f.ty)?,
        op: Some((f, o)),
    })
}

/// Every op of every family, as entries.
pub fn op_entries(cat: &Catalogue) -> Vec<Entry<'_>> {
    family::FAMILIES
        .iter()
        .filter_map(|f| cat.get(f.ty).map(|s| (*f, s)))
        .flat_map(|(f, spec)| {
            f.ops.iter().map(move |o| Entry {
                spec,
                op: Some((f, o)),
            })
        })
        .collect()
}

fn inputs(cat: &Catalogue, s: &NodeSpec, params: &Params) -> Vec<PortSpec> {
    cat.resolved_inputs(s, params)
}

/// The pin on a fresh `spec` node (with `params`) that a `side`/`ty` wire
/// would land on.
pub fn first_compatible(
    cat: &Catalogue,
    s: &NodeSpec,
    params: &Params,
    side: Side,
    ty: PortType,
) -> Option<String> {
    let ports = match side {
        Side::Out => inputs(cat, s, params),
        Side::In => resolved_outputs(cat, s, params),
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
    params: &Params,
    src: PortType,
    dst: PortType,
) -> Option<(String, String)> {
    Some((
        first_compatible(cat, s, params, Side::Out, src)?,
        first_compatible(cat, s, params, Side::In, dst)?,
    ))
}

pub fn fits(cat: &Catalogue, e: &Entry, fit: &Fit) -> bool {
    let params = e.params();
    match fit {
        Fit::Any => true,
        Fit::Pin { side, ty } => first_compatible(cat, e.spec, &params, *side, *ty).is_some(),
        Fit::Insert { src, dst } => insertion_ports(cat, e.spec, &params, *src, *dst).is_some(),
    }
}

/// Entries matching every token of `query`, browsable node types plus (for
/// a non-empty query) the families' ops.
pub fn matching<'a>(cat: &'a Catalogue, query: &str, fit: &Fit) -> Vec<Entry<'a>> {
    let q = query.trim().to_lowercase();
    let tokens: Vec<&str> = q.split_whitespace().collect();
    let nodes = cat.iter().filter(|s| !s.hidden).map(Entry::node);
    let ops = if tokens.is_empty() {
        Vec::new()
    } else {
        op_entries(cat)
    };
    nodes
        .chain(ops)
        .filter(|e| {
            let hay = e.haystack(cat);
            tokens.iter().all(|t| word_match(&hay, t))
        })
        .filter(|e| fits(cat, e, fit))
        .collect()
}

pub fn search<'a>(cat: &'a Catalogue, query: &str, fit: &Fit) -> Vec<Entry<'a>> {
    let q = query.trim().to_lowercase();
    let mut matches = matching(cat, query, fit);
    // An exact label first ("sin" → Sin before SineSpread), then label
    // prefixes, then entries whose names (not just their description)
    // match; nodes before ops; then by label.
    let tokens: Vec<&str> = q.split_whitespace().collect();
    let by_name = |e: &Entry| {
        let names = e.names(cat);
        !tokens.iter().all(|t| word_match(&names, t))
    };
    matches.sort_by_cached_key(|e| {
        let label = e.label().to_lowercase();
        (
            !q.is_empty() && label != q,
            !(q.is_empty() || label.starts_with(&q)),
            by_name(e),
            e.op.is_some(),
            label,
            e.label().to_owned(),
        )
    });
    matches.truncate(MAX_RESULTS);
    matches
}

/// The first sentence of a description, for a one-line hint.
pub fn first_sentence(d: &str) -> &str {
    let end = d
        .char_indices()
        .find(|&(i, c)| c == '.' && d[i + 1..].chars().next().is_none_or(char::is_whitespace))
        .map_or(d.len(), |(i, _)| i + 1);
    d[..end].trim()
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
        for e in &layers {
            assert!(
                first_compatible(cat, e.spec, &e.params(), Side::Out, PortType::Layer).is_some()
            );
        }
        assert!(layers.iter().all(|e| e.spec.ty != "time/seconds"));
        // Multi-token match + prefix-first ordering.
        let r = search(cat, "add", &Fit::Any);
        assert!(r[0].label().to_lowercase().starts_with("add"));
        assert!(search(cat, "zzzz-nothing", &Fit::Any).is_empty());
        // A boolean wire finds the Logic ops preset to and / or …
        let r = search(
            cat,
            "or",
            &Fit::Pin {
                side: Side::Out,
                ty: PortType::Boolean,
            },
        );
        assert!(r.iter().any(|e| e.key() == "logic/logic#or"));
    }

    #[test]
    fn ranking_matches_node_search_tsx() {
        let cat = Catalogue::builtin();
        // Every token must hit label, type, category, group or description,
        // in any order.
        let r = search(cat, "spread ring", &Fit::Any);
        assert!(r.iter().any(|e| e.spec.ty == "spread/ring"));
        for e in &r {
            let h = e.haystack(cat);
            assert!(word_match(&h, "spread") && word_match(&h, "ring"));
        }
        // Label-prefix matches first, then nodes alphabetically by label.
        let r = search(cat, "ring", &Fit::Any);
        let first_other = r
            .iter()
            .position(|e| !e.label().to_lowercase().starts_with("ring"))
            .unwrap_or(r.len());
        assert!(first_other > 0);
        assert!(r[first_other..]
            .iter()
            .all(|e| !e.label().to_lowercase().starts_with("ring")));
        // … name matches before description-only ones, each by label.
        let named = |e: &Entry| word_match(&e.names(cat), "ring");
        let tail = &r[first_other..];
        let split = tail.iter().position(|e| !named(e)).unwrap_or(tail.len());
        assert!(tail[split..].iter().all(|e| !named(e)));
        for part in [&tail[..split], &tail[split..]] {
            let labels: Vec<String> = part
                .iter()
                .filter(|e| e.op.is_none())
                .map(|e| e.label().to_lowercase())
                .collect();
            let mut sorted = labels.clone();
            sorted.sort();
            assert_eq!(labels, sorted);
        }
        // "mul" matches at word starts: Multiply, never "accumulator".
        let r = search(cat, "mul", &Fit::Any);
        assert_eq!(r[0].key(), "math/math#multiply");
        assert!(r.iter().all(|e| !e.spec.ty.contains("accumulator")));
        assert!(search(cat, "", &Fit::Any).len() <= MAX_RESULTS);
        // An empty query lists node types only.
        assert!(search(cat, "", &Fit::Any).iter().all(|e| e.op.is_none()));
    }

    #[test]
    fn old_names_find_the_ops_that_replaced_them() {
        let cat = Catalogue::builtin();
        let first = |q: &str| search(cat, q, &Fit::Any)[0].key();
        assert_eq!(first("mul"), "math/math#multiply");
        assert_eq!(first("sin"), "math/math#sin");
        assert_eq!(first("normalize"), "vec2/math#normalize");
        assert_eq!(first("greater"), "logic/compare#greater");
        assert_eq!(first("xor"), "logic/logic#xor");
        assert_eq!(first("median"), "math/statistics#median");
        // By old type id and old category (all matches, not just the top 40).
        let has = |q: &str, key: &str| matching(cat, q, &Fit::Any).iter().any(|e| e.key() == key);
        assert!(has("math/mul", "math/math#multiply"));
        assert!(has("vec2/dot", "vec2/math#dot"));
        assert!(has("vec2", "vec2/make"));
        assert!(has("vec2 rotate", "vec2/math#rotate"));
        assert!(has("logic/lte", "logic/compare#less-equal"));
        assert!(has("to-string", "convert/convert#string"));
        assert!(has("spread", "spread/ring"));
        assert!(has("geometry", "geometry/scatter"));
        // By new group.
        assert!(has("patterns", "spread/ring"));
        // "math" lists nodes first, not each of Math's ops.
        let r = search(cat, "math", &Fit::Any);
        let nodes = r.iter().take_while(|e| e.op.is_none()).count();
        assert!(nodes >= 5, "{nodes}");
        // Hidden single-op nodes never show up themselves.
        assert!(search(cat, "mul", &Fit::Any).iter().all(|e| !e.spec.hidden));
    }

    #[test]
    fn tokens_match_at_word_starts() {
        assert!(word_match("Multiply", "mul"));
        assert!(!word_match("region/accumulator-read", "mul"));
        assert!(word_match("region/accumulator-read", "read"));
        assert!(word_match("math/mul", "math/mul"));
        assert!(word_match("convert/to-string", "to-string"));
        assert!(word_match("SineSpread", "spread"));
        assert!(word_match("CAGridSeedSpread", "grid"));
        assert!(word_match("vec2", "2"));
        assert!(!word_match("Ring", "ing"));
        assert!(word_match("anything", ""));
    }

    #[test]
    fn renamed_labels_are_still_found_by_their_old_names() {
        let cat = Catalogue::builtin();
        let has = |q: &str, key: &str| matching(cat, q, &Fit::Any).iter().any(|e| e.key() == key);
        assert!(has("sinespread", "spread/sine"));
        assert!(has("sine spread", "spread/sine"));
        assert!(has("SineSpread", "spread/sine"));
        assert!(has("render", "particles/render-as"));
        assert!(has("particle shapes", "particles/render-as"));
    }

    #[test]
    fn keys_round_trip() {
        let cat = Catalogue::builtin();
        let all = op_entries(cat)
            .into_iter()
            .chain(cat.iter().filter(|s| !s.hidden).map(Entry::node));
        for e in all {
            let back = entry(cat, &e.key()).unwrap();
            assert_eq!(back.key(), e.key());
            assert_eq!(back.label(), e.label());
            assert!(e.description(cat).is_some(), "{}", e.key());
        }
        assert_eq!(split_key("math/math#sin"), ("math/math", Some("sin")));
        assert_eq!(split_key("draw/rect"), ("draw/rect", None));
        assert!(entry(cat, "math/math#nope").is_none());
        assert!(entry(cat, "draw/rect#sin").is_none());
        let e = entry(cat, "math/math#multiply").unwrap();
        assert_eq!(e.params()["op"], "multiply");
        assert_eq!(e.owner(), Some("Math"));
        assert_eq!(
            e.description(cat),
            cat.get("math/mul").unwrap().description.as_deref()
        );
        assert_eq!(first_sentence("Sum all. Then more."), "Sum all.");
        assert_eq!(first_sentence("Pick 1.5 or 2"), "Pick 1.5 or 2");
    }

    #[test]
    fn insertion_needs_both_sides() {
        let cat = Catalogue::builtin();
        let fit = Fit::Insert {
            src: PortType::Number,
            dst: PortType::Vec2,
        };
        for e in search(cat, "", &fit) {
            let ports = insertion_ports(cat, e.spec, &e.params(), PortType::Number, PortType::Vec2);
            assert!(ports.is_some());
        }
        let make = search(cat, "vec2/make", &fit);
        assert_eq!(make.len(), 1);
        // Splitting a number → number wire offers Math set to sin.
        let sin = search(
            cat,
            "sin",
            &Fit::Insert {
                src: PortType::Number,
                dst: PortType::Number,
            },
        );
        assert_eq!(sin[0].key(), "math/math#sin");
        // Nothing turns a layer into a number.
        let fit = Fit::Insert {
            src: PortType::Layer,
            dst: PortType::Number,
        };
        assert!(search(cat, "", &fit).iter().all(|e| {
            insertion_ports(cat, e.spec, &e.params(), PortType::Layer, PortType::Number).is_some()
        }));
    }
}
