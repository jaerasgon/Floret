//! `data/*` nodes. Ported from `src/nodes/data/csv.ts`.

use super::value::param_or;
use super::Registrar;
use crate::node::{raw, NodeImpl, NodeInit};
use crate::value::{js_string_to_number, Spread, Value};

pub fn register(r: &mut Registrar) {
    r.node("data/csv-inline", csv_inline);
}

/// JS regex `\s` (WhiteSpace + LineTerminator). Unlike Rust's
/// `char::is_whitespace` it includes U+FEFF and excludes U+0085.
fn js_space(c: char) -> bool {
    ('\u{2000}'..='\u{200A}').contains(&c)
        || matches!(
            c,
            '\t' | '\n'
                | '\u{0B}'
                | '\u{0C}'
                | '\r'
                | ' '
                | '\u{A0}'
                | '\u{1680}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
        )
}

/// Parse `data` into numbers: split on runs of `,` / whitespace, keep the
/// cells `Number()` parses to a finite value.
pub(crate) fn parse_csv(text: &str) -> Vec<Value> {
    text.split(|c: char| c == ',' || js_space(c))
        .filter(|p| !p.is_empty())
        // `Number()` doesn't trim U+0085 (Rust's whitespace does).
        .filter(|p| !p.contains('\u{85}'))
        .map(js_string_to_number)
        .filter(|n| n.is_finite())
        .map(Value::Num)
        .collect()
}

fn csv_inline(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|_, cx| match param_or(cx, "data", Value::str("")) {
        Value::Str(s) => Ok(vec![Spread::from(parse_csv(&s))]),
        // `text.split` on a non-string throws in TS.
        other => Err(format!("text.split is not a function ({other:?})")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nums(s: &str) -> Vec<f64> {
        parse_csv(s).iter().map(|v| v.as_f64().unwrap()).collect()
    }

    #[test]
    fn parses_cells() {
        assert_eq!(
            nums("12, 45,22\n67\t 38"),
            vec![12.0, 45.0, 22.0, 67.0, 38.0]
        );
        assert_eq!(
            nums(",,1e2, x, 0x10, Infinity, -3.5,"),
            vec![100.0, 16.0, -3.5]
        );
        assert_eq!(nums(""), Vec::<f64>::new());
        assert_eq!(nums("1\u{FEFF}2"), vec![1.0, 2.0]);
        assert_eq!(nums("1\u{85}"), Vec::<f64>::new());
    }

    // Expected values recorded from the TS node.
    #[test]
    fn node_matches_ts() {
        use super::super::value::harness::{expect_spreads, node};
        for (data, want) in [
            (
                "12, 45,22\n67\t 38",
                r##"[["#12","#45","#22","#67","#38"]]"##,
            ),
            (
                ",,1e2, x, 0x10, Infinity, -3.5,",
                r##"[["#100","#16","#-3.5"]]"##,
            ),
            ("", "[[]]"),
            ("1;2 3", r##"[["#3"]]"##),
            (" 4 ", r##"[["#4"]]"##),
        ] {
            let mut n = node("data/csv-inline", serde_json::json!({ "data": data }));
            expect_spreads(&n.eval(vec![]).unwrap(), want);
        }
        // A non-string `data` throws in TS (`text.split` is not a function).
        let mut n = node("data/csv-inline", serde_json::json!({ "data": 5 }));
        assert!(n.eval(vec![]).is_err());
    }
}
