//! `data/*` nodes. `csv-inline` is ported from `src/nodes/data/csv.ts`;
//! `csv-file` is native-only (TS deferred it): the same parsing over a
//! loaded file, optionally one column.

use super::shared::first;
use super::value::param_or;
use super::Registrar;
use crate::image::{parse_text, LoadSlot, LoadState};
use crate::node::{raw, NodeImpl, NodeInit};
use crate::value::{js_string_to_number, one, Spread, Value};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("data/csv-inline", csv_inline);
    r.node("data/csv-file", csv_file);
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

/// One CSV line's cells: split on `,` (or `;` / tab when the line has no
/// comma), quotes stripped.
fn cells(line: &str) -> impl Iterator<Item = &str> {
    let sep = if line.contains(',') {
        ','
    } else if line.contains(';') {
        ';'
    } else {
        '\t'
    };
    line.split(sep)
        .map(|c| c.trim_matches(js_space).trim_matches('"'))
}

/// The finite numbers in column `column` of each row, or (`column < 0`)
/// every cell like `csv-inline`; and the number of non-empty rows.
pub(crate) fn parse_csv_column(text: &str, column: i64) -> (Vec<Value>, usize) {
    let text = text.trim_start_matches('\u{feff}');
    let rows = text.lines().filter(|l| !l.trim().is_empty()).count();
    if column < 0 {
        return (parse_csv(text), rows);
    }
    let values = text
        .lines()
        .filter_map(|l| cells(l).nth(column as usize))
        .filter(|c| !c.is_empty())
        .map(js_string_to_number)
        .filter(|n| n.is_finite())
        .map(Value::Num)
        .collect();
    (values, rows)
}

/// `data/csv-file`: inputs (url, column), outputs (values, rows, loaded).
/// The url is a path (relative to the patch's folder), an `http(s)` URL or
/// a `data:` URL such as `io/file`'s.
fn csv_file(_: &NodeInit) -> Box<dyn NodeImpl> {
    let mut slot: LoadSlot<Arc<str>> = LoadSlot::default();
    let mut parsed: Option<(Arc<str>, i64, Spread, usize)> = None;
    raw(move |inputs, cx| {
        let url = match first(inputs, 0) {
            Value::Str(s) => s.trim().to_owned(),
            _ => String::new(),
        };
        let column = match first(inputs, 1).to_number() {
            c if c.is_finite() => c.floor() as i64,
            _ => -1,
        };
        let empty = |loaded: bool| {
            Ok(vec![
                Spread::from(Vec::new()),
                one(Value::Num(0.0)),
                one(loaded.into()),
            ])
        };
        if url.is_empty() {
            slot.clear();
            parsed = None;
            return empty(false);
        }
        let text = match slot.ensure(&url, cx.host(), parse_text).state() {
            LoadState::Ready(t) => t,
            LoadState::Failed(e) => {
                cx.warn(format!("Couldn't load the CSV: {e}"));
                return empty(false);
            }
            LoadState::Loading => return empty(false),
        };
        let fresh = parsed
            .as_ref()
            .is_some_and(|(t, c, ..)| Arc::ptr_eq(t, &text) && *c == column);
        if !fresh {
            let (values, rows) = parse_csv_column(&text, column);
            parsed = Some((text, column, Spread::from(values), rows));
        }
        let (_, _, values, rows) = parsed.as_ref().expect("parsed");
        Ok(vec![
            values.clone(),
            one(Value::Num(*rows as f64)),
            one(true.into()),
        ])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_columns() {
        let csv = "\u{feff}year,count\n2020,\"12\"\n2021, 15\n\n2022,x\n";
        let col = |c| {
            let (v, rows) = parse_csv_column(csv, c);
            (
                v.iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>(),
                rows,
            )
        };
        assert_eq!(col(1), (vec![12.0, 15.0], 4));
        assert_eq!(col(0), (vec![2020.0, 2021.0, 2022.0], 4));
        assert_eq!(col(5), (vec![], 4));
        let semi = parse_csv_column("1;2\n3;4", 1).0;
        assert_eq!(
            semi.iter().map(Value::as_f64).collect::<Vec<_>>(),
            [Some(2.0), Some(4.0)]
        );
        // −1: every numeric cell, like csv-inline.
        assert_eq!(col(-1).0, vec![2020.0, 2021.0, 15.0, 2022.0]);
    }

    #[test]
    fn csv_file_loads_relative_to_the_host_folder() {
        use super::super::value::harness::node;
        struct Folder(std::path::PathBuf);
        impl crate::host::Host for Folder {
            fn base_dir(&self) -> Option<std::path::PathBuf> {
                Some(self.0.clone())
            }
        }
        let dir = std::env::temp_dir().join(format!("floret-csv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("d.csv"), "a,b\n1,2\n3,4\n").unwrap();
        let host: Arc<dyn crate::host::Host> = Arc::new(Folder(dir.clone()));
        let _g = crate::host::install(Some(host));
        let mut n = node("data/csv-file", serde_json::json!({}));
        let args = |url: &str| vec![vec![Value::str(url)], vec![Value::Num(1.0)]];
        let mut out = n.eval(args("d.csv")).unwrap();
        for _ in 0..500 {
            if matches!(out[2][0], Value::Bool(true)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            out = n.eval(args("d.csv")).unwrap();
        }
        let nums = |s: &[Value]| s.iter().map(Value::as_f64).collect::<Vec<_>>();
        assert_eq!(nums(&out[0]), [Some(2.0), Some(4.0)]);
        assert_eq!(nums(&out[1]), [Some(3.0)]);
        // A missing file: not loaded, and a warning for the node.
        let _ = crate::node::take_warnings();
        let mut out = n.eval(args("missing.csv")).unwrap();
        for _ in 0..500 {
            if !crate::node::take_warnings().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            out = n.eval(args("missing.csv")).unwrap();
        }
        assert!(matches!(out[2][..], [Value::Bool(false)]));
        assert!(out[0].is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

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
