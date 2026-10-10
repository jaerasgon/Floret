//! `spread/bin` (Lookup's `bin` op): for each of `values`, the band it
//! falls in between the `spread` thresholds — a step function, like a
//! histogram's bin index. Noise → biome bands, a value → its colour class.
//!
//! A native-only node. Band `k` holds the values with exactly `k`
//! thresholds at or below them: below the lowest threshold is band 0, at or
//! above the highest is band `len`. Thresholds needn't be sorted (a sorted
//! copy is searched); non-numbers and NaNs among them are ignored. A NaN
//! (or non-number) value is in no band: −1.

use super::{ok, sp};
use crate::node::{EvalCtx, NodeResult};
use crate::value::{Spread, Value};

pub fn bin(inputs: &[Spread], _: &mut EvalCtx) -> NodeResult {
    let mut edges: Vec<f64> = sp(inputs, 0)
        .iter()
        .filter_map(Value::as_f64)
        .filter(|x| !x.is_nan())
        .collect();
    edges.sort_by(f64::total_cmp);
    let out = sp(inputs, 1)
        .iter()
        .map(|v| Value::Num(band(&edges, v)))
        .collect();
    ok(vec![out])
}

/// The band index of `v` among sorted `edges`.
fn band(edges: &[f64], v: &Value) -> f64 {
    let x = match v {
        Value::Num(x) => *x,
        Value::Bool(b) => f64::from(u8::from(*b)),
        _ => return -1.0,
    };
    if x.is_nan() {
        return -1.0;
    }
    edges.partition_point(|e| *e <= x) as f64
}

#[cfg(test)]
mod tests {
    use crate::nodes::value::harness::{n, node, s};
    use crate::value::Value;
    use serde_json::json;

    fn bins(edges: &[Value], values: &[Value]) -> Vec<f64> {
        let mut h = node("spread/lookup", json!({ "op": "bin" }));
        h.eval(vec![edges.to_vec(), values.to_vec()]).unwrap()[0]
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect()
    }

    #[test]
    fn values_fall_into_bands() {
        let edges = [n(0.3), n(0.5), n(0.8)];
        let vals = [n(-1.0), n(0.3), n(0.31), n(0.5), n(0.79), n(0.8), n(5.0)];
        assert_eq!(bins(&edges, &vals), [0.0, 1.0, 1.0, 2.0, 2.0, 3.0, 3.0]);
    }

    #[test]
    fn unsorted_and_junk_thresholds() {
        let edges = [n(0.8), s("x"), n(f64::NAN), n(0.3), Value::Null];
        assert_eq!(bins(&edges, &[n(0.1), n(0.5), n(0.9)]), [0.0, 1.0, 2.0]);
    }

    #[test]
    fn edge_cases() {
        // No thresholds: one band.
        assert_eq!(bins(&[], &[n(3.0), n(-3.0)]), [0.0, 0.0]);
        // No values: nothing out.
        assert!(bins(&[n(1.0)], &[]).is_empty());
        // NaN and non-numbers are in no band; infinities in the end bands.
        assert_eq!(
            bins(
                &[n(0.0)],
                &[n(f64::NAN), s("a"), n(f64::INFINITY), n(f64::NEG_INFINITY)]
            ),
            [-1.0, -1.0, 1.0, 0.0]
        );
        // Repeated thresholds make an empty band in between.
        assert_eq!(bins(&[n(1.0), n(1.0)], &[n(0.5), n(1.0)]), [0.0, 2.0]);
    }

    #[test]
    fn bin_op_has_the_lookup_pins() {
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("spread/lookup").unwrap();
        let p: crate::spec::Params = serde_json::from_value(json!({ "op": "bin" })).unwrap();
        let names: Vec<String> = cat
            .resolved_inputs(spec, &p)
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["spread", "values"]);
        let outs = cat.resolved_outputs(spec, &p);
        assert_eq!(outs[0].name, "result");
    }
}
