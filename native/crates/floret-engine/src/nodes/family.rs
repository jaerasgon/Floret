//! Operator families: one node per concept with an op dropdown — Math,
//! Vector Math, Compare, Logic, Statistics and Convert, and the list
//! families Lookup, Sublist, Reorder and Combine — in place of one node per
//! operator.
//!
//! Each op *is* the single-op node it supersedes (`math/mul`, `vec2/dot`,
//! `logic/lt` …): its pins are that node's (dynamic) inputs, its output
//! that node's output renamed `result`, and it computes with that node's
//! own factory. So a family node set to an op gives exactly the old node's
//! results, spread handling and edge cases included. The single-op nodes
//! stay loadable (old patches, the parity fixtures) but are hidden from
//! the browser.

use super::Registrar;
use crate::node::{scalar, Factory, NodeImpl, NodeInit};
use crate::spec::{Catalogue, NodeSpec, Params, PortSpec};
use crate::value::Value;

/// One choice in a family's dropdown.
#[derive(Debug)]
pub struct Op {
    /// The stored param value.
    pub id: &'static str,
    /// Human name (search results, help).
    pub label: &'static str,
    /// The canvas header for math-style families (`×`, `<` …).
    pub symbol: Option<&'static str>,
    /// The single-op node whose pins, output and compute this op uses.
    pub node: &'static str,
    /// A compute of its own, for an op no single-op node had (xor); the
    /// pins still come from `node`.
    pub compute: Option<Factory>,
}

impl Op {
    /// The superseded node type, when the op computes exactly as it.
    pub fn legacy(&self) -> Option<&'static str> {
        self.compute.is_none().then_some(self.node)
    }
}

/// A node type whose behaviour is picked by an op dropdown.
#[derive(Debug)]
pub struct Family {
    pub ty: &'static str,
    /// The dropdown's param.
    pub param: &'static str,
    /// In dropdown order; the first is the default.
    pub ops: &'static [Op],
    /// Ops whose pins adapt to number / vec2 / colour (`valueType`).
    pub adaptive: &'static [&'static str],
}

impl Family {
    /// The op these params select (the default for a missing or unknown
    /// value, so a newer file's op never breaks loading).
    pub fn op(&self, params: &Params) -> &'static Op {
        let id = params.get(self.param).and_then(serde_json::Value::as_str);
        self.ops
            .iter()
            .find(|o| Some(o.id) == id)
            .unwrap_or(&self.ops[0])
    }

    pub fn op_by_id(&self, id: &str) -> Option<&'static Op> {
        self.ops.iter().find(|o| o.id == id)
    }

    fn op_spec(&self, params: &Params) -> (&'static Op, &'static NodeSpec) {
        let op = self.op(params);
        let spec = Catalogue::builtin()
            .get(op.node)
            .unwrap_or_else(|| panic!("{}: no spec for {}", self.ty, op.node));
        (op, spec)
    }

    fn inputs(&self, params: &Params) -> Vec<PortSpec> {
        let (_, spec) = self.op_spec(params);
        Catalogue::builtin().resolved_inputs(spec, params)
    }

    fn outputs(&self, params: &Params) -> Vec<PortSpec> {
        let (_, spec) = self.op_spec(params);
        spec.outputs
            .iter()
            .take(1)
            .map(|p| PortSpec {
                name: "result".into(),
                ..p.clone()
            })
            .collect()
    }

    fn adapts(&self, params: &Params) -> bool {
        self.adaptive.contains(&self.op(params).id)
    }

    fn build(&self, init: &NodeInit) -> Box<dyn NodeImpl> {
        let (op, spec) = self.op_spec(init.params);
        let factory = op
            .compute
            .or_else(|| super::factory(op.node))
            .unwrap_or_else(|| panic!("{}: no factory for {}", self.ty, op.node));
        factory(&NodeInit {
            id: init.id,
            spec,
            params: init.params,
            inputs: init.inputs,
        })
    }
}

const fn op(id: &'static str, label: &'static str, node: &'static str) -> Op {
    Op {
        id,
        label,
        symbol: None,
        node,
        compute: None,
    }
}

const fn sym(
    id: &'static str,
    label: &'static str,
    symbol: &'static str,
    node: &'static str,
) -> Op {
    Op {
        id,
        label,
        symbol: Some(symbol),
        node,
        compute: None,
    }
}

pub static MATH: Family = Family {
    ty: "math/math",
    param: "op",
    ops: &[
        sym("add", "Add", "+", "math/add"),
        sym("subtract", "Subtract", "−", "math/sub"),
        sym("multiply", "Multiply", "×", "math/mul"),
        sym("divide", "Divide", "÷", "math/div"),
        sym("mod", "Modulo", "mod", "math/mod"),
        sym("pow", "Power", "x^y", "math/pow"),
        sym("min", "Min", "min", "math/min"),
        sym("max", "Max", "max", "math/max"),
        sym("negate", "Negate", "−x", "math/neg"),
        sym("abs", "Absolute", "|x|", "math/abs"),
        sym("sign", "Sign", "sign", "math/sign"),
        sym("floor", "Floor", "⌊x⌋", "math/floor"),
        sym("ceil", "Ceil", "⌈x⌉", "math/ceil"),
        sym("round", "Round", "⌊x⌉", "math/round"),
        sym("fract", "Fract", "{x}", "math/fract"),
        sym("sqrt", "Square Root", "√", "math/sqrt"),
        sym("square", "Square", "x²", "math/sq"),
        sym("exp", "Exp", "e^x", "math/exp"),
        sym("log", "Log", "ln", "math/log"),
        sym("sin", "Sin", "sin", "math/sin"),
        sym("cos", "Cos", "cos", "math/cos"),
        sym("tan", "Tan", "tan", "math/tan"),
        sym("asin", "Asin", "asin", "math/asin"),
        sym("acos", "Acos", "acos", "math/acos"),
        sym("atan", "Atan", "atan", "math/atan"),
        sym("atan2", "Atan2", "atan2", "math/atan2"),
        sym("length", "Length", "‖v‖", "math/mag"),
        sym("distance", "Distance", "dist", "math/dist"),
    ],
    adaptive: &[
        "add", "subtract", "multiply", "divide", "min", "max", "negate", "abs",
    ],
};

pub static VECTOR: Family = Family {
    ty: "vec2/math",
    param: "op",
    ops: &[
        op("length", "Vector Length", "vec2/mag"),
        op("distance", "Vector Distance", "vec2/dist"),
        op("dot", "Dot Product", "vec2/dot"),
        op("angle-between", "Angle Between", "vec2/angle-between"),
        op("heading", "Heading", "vec2/heading"),
        op("normalize", "Normalize", "vec2/normalize"),
        op("set-length", "Set Length", "vec2/set-mag"),
        op("limit", "Limit Length", "vec2/limit"),
        op("scale", "Scale Vector", "vec2/scale"),
        op("rotate", "Rotate Vector", "vec2/rotate"),
        op("project", "Project", "vec2/project"),
        op("reflect", "Reflect", "vec2/reflect"),
    ],
    adaptive: &[],
};

pub static COMPARE: Family = Family {
    ty: "logic/compare",
    param: "op",
    ops: &[
        sym("less", "Less", "<", "logic/lt"),
        sym("less-equal", "Less or Equal", "≤", "logic/lte"),
        sym("greater", "Greater", ">", "logic/gt"),
        sym("greater-equal", "Greater or Equal", "≥", "logic/gte"),
        sym("equal", "Equal", "=", "logic/eq"),
        sym("not-equal", "Not Equal", "≠", "logic/neq"),
    ],
    adaptive: &[],
};

pub static LOGIC: Family = Family {
    ty: "logic/logic",
    param: "op",
    ops: &[
        sym("and", "And", "and", "logic/and"),
        sym("or", "Or", "or", "logic/or"),
        Op {
            id: "xor",
            label: "Xor",
            symbol: Some("xor"),
            node: "logic/and",
            compute: Some(xor),
        },
        sym("not", "Not", "not", "logic/not"),
    ],
    adaptive: &[],
};

pub static STATISTICS: Family = Family {
    ty: "math/statistics",
    param: "op",
    ops: &[
        op("mean", "Mean", "math/mean"),
        op("median", "Median", "math/median"),
        op("sum", "Sum", "math/sum"),
        op("min", "Min of Spread", "math/min-spread"),
        op("max", "Max of Spread", "math/max-spread"),
        op("stddev", "Standard Deviation", "math/stddev"),
    ],
    adaptive: &[],
};

pub static CONVERT: Family = Family {
    ty: "convert/convert",
    param: "to",
    ops: &[
        op("number", "To Number", "convert/to-number"),
        op("integer", "To Integer", "convert/to-int"),
        op("boolean", "To Boolean", "convert/to-bool"),
        op("string", "To String", "convert/to-string"),
        op("radians", "To Radians", "convert/to-radians"),
        op("degrees", "To Degrees", "convert/to-degrees"),
    ],
    adaptive: &[],
};

/// Read from a list: by index, where a value is, the nearest point, or the
/// band a value falls in.
pub static LOOKUP: Family = Family {
    ty: "spread/lookup",
    param: "op",
    ops: &[
        op("at", "Item At", "spread/at"),
        op("pick", "Items At", "spread/pick"),
        op("group-at", "Group At", "spread/at-group"),
        op("index-of", "Index Of", "spread/index-of"),
        op("indices-of", "Indices Of", "spread/indices-by-value"),
        op("nearest", "Nearest", "spread/nearest"),
        op("bin", "Bin", "spread/bin"),
    ],
    adaptive: &[],
};

/// Keep part of a list, in order.
pub static SUBLIST: Family = Family {
    ty: "spread/sublist",
    param: "op",
    ops: &[
        op("take", "Take", "spread/take"),
        op("skip", "Skip", "spread/skip"),
        op("slice", "Slice", "spread/slice"),
        op("remove", "Remove Range", "spread/remove-slice"),
        op("keep-where", "Keep Where", "spread/sift"),
        op("unique", "Unique", "spread/unique"),
    ],
    adaptive: &[],
};

/// The same items in a new order.
pub static REORDER: Family = Family {
    ty: "spread/reorder",
    param: "op",
    ops: &[
        op("reverse", "Reverse", "spread/reverse"),
        op("rotate", "Rotate List", "spread/rotate"),
        op("sort", "Sort", "spread/sort"),
        op("sort-by-key", "Sort By Key", "spread/sort-by-key"),
        op("permute", "Permute", "spread/permute"),
        op("swap", "Swap", "spread/swap"),
    ],
    adaptive: &[],
};

/// Put items or lists together.
pub static COMBINE: Family = Family {
    ty: "spread/combine",
    param: "op",
    ops: &[
        op("append", "Append", "spread/append"),
        op("prepend", "Prepend", "spread/cons"),
        op("concat", "Concatenate", "spread/stack"),
        op("interleave", "Interleave", "spread/interleave"),
        op("insert", "Insert", "spread/insert-slice"),
        op("replace", "Replace At", "spread/set-slice"),
        op("inject", "Inject", "spread/inject"),
    ],
    adaptive: &[],
};

/// Every family, in browser order.
pub static FAMILIES: [&Family; 10] = [
    &MATH,
    &VECTOR,
    &COMPARE,
    &LOGIC,
    &STATISTICS,
    &CONVERT,
    &LOOKUP,
    &SUBLIST,
    &REORDER,
    &COMBINE,
];

/// The family a node type is, if any.
pub fn family(ty: &str) -> Option<&'static Family> {
    FAMILIES.iter().copied().find(|f| f.ty == ty)
}

/// The family node and op that supersede a hidden single-op node.
pub fn superseded_by(ty: &str) -> Option<(&'static Family, &'static Op)> {
    FAMILIES.iter().find_map(|f| {
        f.ops
            .iter()
            .find(|o| o.legacy() == Some(ty))
            .map(|o| (*f, o))
    })
}

/// `a xor b`: exactly one truthy (the truthiness of And / Or).
fn xor(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| {
        Ok(vec![Value::Bool(a[0].truthy() != a[1].truthy())])
    })
}

pub fn register(r: &mut Registrar) {
    // Factories and dynamic-pin functions are plain `fn`s: one closure per
    // family, each naming its static.
    r.node("math/math", |i| MATH.build(i));
    r.dynamic("math/math", |p| MATH.inputs(p));
    r.dynamic_outputs("math/math", |p| MATH.outputs(p));
    r.adaptive_when("math/math", |p| MATH.adapts(p));

    r.node("vec2/math", |i| VECTOR.build(i));
    r.dynamic("vec2/math", |p| VECTOR.inputs(p));
    r.dynamic_outputs("vec2/math", |p| VECTOR.outputs(p));

    r.node("logic/compare", |i| COMPARE.build(i));
    r.dynamic("logic/compare", |p| COMPARE.inputs(p));
    r.dynamic_outputs("logic/compare", |p| COMPARE.outputs(p));

    r.node("logic/logic", |i| LOGIC.build(i));
    r.dynamic("logic/logic", |p| LOGIC.inputs(p));
    r.dynamic_outputs("logic/logic", |p| LOGIC.outputs(p));

    r.node("math/statistics", |i| STATISTICS.build(i));
    r.dynamic("math/statistics", |p| STATISTICS.inputs(p));
    r.dynamic_outputs("math/statistics", |p| STATISTICS.outputs(p));

    r.node("convert/convert", |i| CONVERT.build(i));
    // Per element over spreads (v4 patches on): every op of these families
    // is a lifting scalar node.
    r.lifted(&[
        "math/math",
        "vec2/math",
        "logic/compare",
        "logic/logic",
        "convert/convert",
    ]);
    r.dynamic("convert/convert", |p| CONVERT.inputs(p));
    r.dynamic_outputs("convert/convert", |p| CONVERT.outputs(p));

    r.node("spread/lookup", |i| LOOKUP.build(i));
    r.dynamic("spread/lookup", |p| LOOKUP.inputs(p));
    r.dynamic_outputs("spread/lookup", |p| LOOKUP.outputs(p));

    r.node("spread/sublist", |i| SUBLIST.build(i));
    r.dynamic("spread/sublist", |p| SUBLIST.inputs(p));
    r.dynamic_outputs("spread/sublist", |p| SUBLIST.outputs(p));

    r.node("spread/reorder", |i| REORDER.build(i));
    r.dynamic("spread/reorder", |p| REORDER.inputs(p));
    r.dynamic_outputs("spread/reorder", |p| REORDER.outputs(p));

    r.node("spread/combine", |i| COMBINE.build(i));
    r.dynamic("spread/combine", |p| COMBINE.inputs(p));
    r.dynamic_outputs("spread/combine", |p| COMBINE.outputs(p));
}

#[cfg(test)]
mod tests;
