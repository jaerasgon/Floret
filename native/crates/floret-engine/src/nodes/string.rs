//! `string/*` nodes. Ported from `src/nodes/value/string-ops.ts`.
//!
//! "Characters" are code points (`Array.from(text)`), which is exactly a
//! Rust `char`; plain `.length` would be UTF-16 units.

use super::convert::js_string_or_empty;
use super::shared::first;
use super::Registrar;
use crate::node::{raw, scalar, NodeImpl, NodeInit};
use crate::value::{one, Spread, Value};

pub fn register(r: &mut Registrar) {
    r.node("string/split", split);
    r.node("string/concat", concat);
    r.node("string/word-indices", word_indices);
    r.node("string/length", length);
}

fn spread(items: Vec<Value>) -> Spread {
    Spread::from(items)
}

/// `by = ""` → one item per character, else `text.split(by)`.
fn split(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let text = js_string_or_empty(&first(inputs, 0));
        let by = js_string_or_empty(&first(inputs, 1));
        let out: Vec<Value> = if by.is_empty() {
            text.chars()
                .map(|c| Value::str(c.encode_utf8(&mut [0; 4])))
                .collect()
        } else {
            text.split(by.as_str()).map(Value::str).collect()
        };
        Ok(vec![spread(out)])
    })
}

fn concat(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let parts: Vec<String> = inputs
            .first()
            .map(|s| s.iter().map(js_string_or_empty).collect())
            .unwrap_or_default();
        let sep = js_string_or_empty(&first(inputs, 1));
        Ok(vec![one(Value::str(&parts.join(&sep)))])
    })
}

fn word_indices(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let text = js_string_or_empty(&first(inputs, 0));
        let by = match first(inputs, 1) {
            v if v.is_nullish() => " ".to_owned(),
            v => js_string_or_empty(&v),
        };
        let mut out = Vec::new();
        let mut word = 0.0;
        let mut in_word = false;
        for c in text.chars() {
            out.push(Value::Num(word));
            if !by.is_empty() && by.contains(c) {
                if in_word {
                    word += 1.0;
                    in_word = false;
                }
            } else {
                in_word = true;
            }
        }
        Ok(vec![spread(out)])
    })
}

fn length(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |a| {
        Ok(vec![Value::Num(
            js_string_or_empty(&a[0]).chars().count() as f64
        )])
    })
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{expect, expect_spreads, list, n, node, obj, s};
    use crate::value::Value;

    fn raw(ty: &str, inputs: Vec<Vec<Value>>, want: &str) {
        let got = node(ty, serde_json::json!({})).eval(inputs).unwrap();
        expect_spreads(&got, want);
    }

    // Expected values recorded from the TS nodes.
    #[test]
    fn split_matches_ts() {
        let t = "string/split";
        raw(
            t,
            vec![vec![s("hello world")], vec![s(" ")]],
            r#"[["hello","world"]]"#,
        );
        raw(
            t,
            vec![vec![s("h\u{e9}llo\u{1F600}")], vec![s("")]],
            r#"[["h","é","l","l","o","😀"]]"#,
        );
        raw(
            t,
            vec![vec![s("a,b,,c,")], vec![s(",")]],
            r#"[["a","b","","c",""]]"#,
        );
        raw(t, vec![vec![], vec![]], "[[]]");
        raw(t, vec![vec![n(12.5)], vec![s(".")]], r#"[["12","5"]]"#);
        raw(t, vec![vec![Value::Null], vec![Value::Undefined]], "[[]]");
        raw(
            t,
            vec![vec![list(&[n(1.0), n(2.0), n(3.0)])], vec![s(",")]],
            r#"[["1","2","3"]]"#,
        );
    }

    #[test]
    fn concat_matches_ts() {
        let t = "string/concat";
        let parts = vec![
            s("a"),
            n(1.0),
            Value::Null,
            Value::Undefined,
            Value::Bool(true),
            n(2.5),
        ];
        raw(t, vec![parts, vec![s("-")]], r#"[["a-1---true-2.5"]]"#);
        raw(t, vec![vec![], vec![]], r#"[[""]]"#);
        let parts = vec![
            obj(&[("x", n(1.0)), ("y", n(2.0))]),
            list(&[n(1.0), list(&[n(2.0), n(3.0)])]),
        ];
        raw(
            t,
            vec![parts, vec![Value::Null]],
            r#"[["[object Object]1,2,3"]]"#,
        );
    }

    #[test]
    fn word_indices_match_ts() {
        let t = "string/word-indices";
        raw(
            t,
            vec![vec![s("hello  world foo")], vec![s(" ")]],
            r##"[["#0","#0","#0","#0","#0","#0","#1","#1","#1","#1","#1","#1","#1","#2","#2","#2"]]"##,
        );
        raw(
            t,
            vec![vec![s("a,b c,,d")], vec![s(", ")]],
            r##"[["#0","#0","#1","#1","#2","#2","#3","#3"]]"##,
        );
        raw(
            t,
            vec![vec![s("abc")], vec![s("")]],
            r##"[["#0","#0","#0"]]"##,
        );
        raw(t, vec![vec![s("")], vec![s(" ")]], "[[]]");
        raw(
            t,
            vec![vec![s(" lead\u{1F600} x ")], vec![]],
            r##"[["#0","#0","#0","#0","#0","#0","#0","#1","#1"]]"##,
        );
        raw(
            t,
            vec![vec![s("a1b22c")], vec![n(12.0)]],
            r##"[["#0","#0","#1","#1","#2","#2"]]"##,
        );
    }

    #[test]
    fn length_matches_ts() {
        let mut len = node("string/length", serde_json::json!({}));
        for (v, want) in [
            (s("h\u{e9}llo\u{1F600}"), r##"["#6"]"##),
            (Value::Null, r##"["#0"]"##),
            (Value::Undefined, r##"["#0"]"##),
            (n(123.0), r##"["#3"]"##),
            (s(""), r##"["#0"]"##),
            (list(&[n(1.0), n(2.0)]), r##"["#3"]"##),
        ] {
            expect(&len.eval1(&[v]), want);
        }
    }
}
