//! `math/expression`: a formula typed on the node, e.g. `sin(x * 3) + y * t`.
//!
//! A native-only node (issue #45). Every free variable in the formula
//! becomes a spread-kind input pin, so the formula runs once per element
//! (shorter spreads cycle). `t` and `frame` read the frame clock, `index`
//! and `count` the element being computed; `pi`, `tau` and `e` are
//! constants. Values are numbers, vec2s or colours: `p.x`, `c.r` read a
//! component (a variable read that way gets a vec2 / colour pin), and
//! arithmetic works component-wise, with numbers broadcast.
//!
//! The language is a small hand-written precedence-climbing parser over a
//! tree-walking evaluator: arithmetic, comparisons, `&&` / `||` / `!`, the
//! ternary `c ? a : b` and a fixed table of pure functions. There are no
//! loops, assignments or IO, the source and its nesting are capped, so the
//! work per element is bounded by the formula's size.

use super::color::hsl_to_color01;
use super::math::adaptive::{kind_of, Kind};
use super::noise::{node_seed, perlin_2d};
use super::shared::{cyclic, cyclic_len};
use super::Registrar;
use crate::js;
use crate::node::{EvalCtx, NodeImpl, NodeInit, NodeResult};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{Color, Spread, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("math/expression", expression);
    r.dynamic("math/expression", expression_inputs);
}

/// The formula a new node starts with.
const DEFAULT_EXPR: &str = "sin(x * 3) + y * t";
/// Longer sources are refused (a formula, not a program).
const MAX_SOURCE: usize = 4096;
/// Deeper nesting is refused (keeps the recursive parser and evaluator
/// well inside the stack).
const MAX_DEPTH: usize = 64;

// ─── Lexer ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    /// An operator or punctuation: `+ - * / % ^ ( ) , . ? : ! < <= > >=
    /// == != && ||`.
    Sym(&'static str),
}

#[derive(Clone, Debug)]
struct Token {
    tok: Tok,
    /// 1-based column, for error messages.
    col: usize,
}

const SYMBOLS: [&str; 21] = [
    "<=", ">=", "==", "!=", "&&", "||", "+", "-", "*", "/", "%", "^", "(", ")", ",", ".", "?", ":",
    "!", "<", ">",
];

fn lex(src: &str) -> Result<Vec<Token>, String> {
    lex_with(src, false)
}

/// Tokens; with `lossy`, characters that don't lex are skipped instead
/// of failing.
fn lex_with(src: &str, lossy: bool) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let col = i + 1;
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            // An exponent: `1e3`, `2.5E-4`.
            if i < chars.len() && matches!(chars[i], 'e' | 'E') {
                let mut j = i + 1;
                if j < chars.len() && matches!(chars[j], '+' | '-') {
                    j += 1;
                }
                if j < chars.len() && chars[j].is_ascii_digit() {
                    i = j;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let text: String = chars[start..i].iter().collect();
            match text.parse::<f64>() {
                Ok(n) => out.push(Token {
                    tok: Tok::Num(n),
                    col,
                }),
                Err(_) if lossy => {}
                Err(_) => return Err(format!("\"{text}\" isn't a number (column {col})")),
            }
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Ident(chars[start..i].iter().collect()),
                col,
            });
        } else {
            let rest: String = chars[i..chars.len().min(i + 2)].iter().collect();
            let Some(sym) = SYMBOLS.iter().find(|s| rest.starts_with(**s)) else {
                if lossy {
                    i += 1;
                    continue;
                }
                return Err(format!("unexpected \"{c}\" (column {col})"));
            };
            i += sym.chars().count();
            out.push(Token {
                tok: Tok::Sym(sym),
                col,
            });
        }
    }
    Ok(out)
}

// ─── Syntax tree ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Builtin {
    Time,
    Frame,
    Index,
    Count,
}

/// Names that never become pins.
fn builtin(name: &str) -> Option<Builtin> {
    Some(match name {
        "t" => Builtin::Time,
        "frame" => Builtin::Frame,
        "index" => Builtin::Index,
        "count" => Builtin::Count,
        _ => return None,
    })
}

fn constant(name: &str) -> Option<f64> {
    Some(match name {
        "pi" | "PI" => std::f64::consts::PI,
        "tau" | "TAU" => std::f64::consts::TAU,
        "e" | "E" => std::f64::consts::E,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

/// Binding power of the infix operators handled by precedence climbing
/// (`^` binds tighter than unary minus and is parsed separately).
fn infix(sym: &str) -> Option<(BinOp, u8)> {
    Some(match sym {
        "||" => (BinOp::Or, 1),
        "&&" => (BinOp::And, 2),
        "==" => (BinOp::Eq, 3),
        "!=" => (BinOp::Ne, 3),
        "<" => (BinOp::Lt, 4),
        "<=" => (BinOp::Le, 4),
        ">" => (BinOp::Gt, 4),
        ">=" => (BinOp::Ge, 4),
        "+" => (BinOp::Add, 5),
        "-" => (BinOp::Sub, 5),
        "*" => (BinOp::Mul, 6),
        "/" => (BinOp::Div, 6),
        "%" => (BinOp::Mod, 6),
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Comp {
    X,
    Y,
    R,
    G,
    B,
    A,
}

impl Comp {
    fn parse(s: &str) -> Option<Comp> {
        Some(match s {
            "x" => Comp::X,
            "y" => Comp::Y,
            "r" => Comp::R,
            "g" => Comp::G,
            "b" => Comp::B,
            "a" => Comp::A,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Comp::X => "x",
            Comp::Y => "y",
            Comp::R => "r",
            Comp::G => "g",
            Comp::B => "b",
            Comp::A => "a",
        }
    }

    /// The pin type a variable read with this component gets.
    fn port_type(self) -> PortType {
        match self {
            Comp::X | Comp::Y => PortType::Vec2,
            _ => PortType::Color,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Func {
    // One number in, applied per component.
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Exp,
    Log,
    Log2,
    Log10,
    Sqrt,
    Abs,
    Floor,
    Ceil,
    Round,
    Fract,
    Sign,
    Trunc,
    Radians,
    Degrees,
    // Several arguments.
    Atan2,
    Pow,
    Min,
    Max,
    Mod,
    Step,
    Hypot,
    Clamp,
    Mix,
    Smoothstep,
    Map,
    Noise,
    // Vectors and colours.
    Vec2,
    Length,
    Distance,
    Dot,
    Normalize,
    Angle,
    Rgb,
    Hsl,
}

/// (name, function, min args, max args).
const FUNCS: &[(&str, Func, usize, usize)] = &[
    ("sin", Func::Sin, 1, 1),
    ("cos", Func::Cos, 1, 1),
    ("tan", Func::Tan, 1, 1),
    ("asin", Func::Asin, 1, 1),
    ("acos", Func::Acos, 1, 1),
    ("atan", Func::Atan, 1, 1),
    ("sinh", Func::Sinh, 1, 1),
    ("cosh", Func::Cosh, 1, 1),
    ("tanh", Func::Tanh, 1, 1),
    ("exp", Func::Exp, 1, 1),
    ("log", Func::Log, 1, 1),
    ("ln", Func::Log, 1, 1),
    ("log2", Func::Log2, 1, 1),
    ("log10", Func::Log10, 1, 1),
    ("sqrt", Func::Sqrt, 1, 1),
    ("abs", Func::Abs, 1, 1),
    ("floor", Func::Floor, 1, 1),
    ("ceil", Func::Ceil, 1, 1),
    ("round", Func::Round, 1, 1),
    ("fract", Func::Fract, 1, 1),
    ("sign", Func::Sign, 1, 1),
    ("trunc", Func::Trunc, 1, 1),
    ("radians", Func::Radians, 1, 1),
    ("degrees", Func::Degrees, 1, 1),
    ("atan2", Func::Atan2, 2, 2),
    ("pow", Func::Pow, 2, 2),
    ("min", Func::Min, 2, 16),
    ("max", Func::Max, 2, 16),
    ("mod", Func::Mod, 2, 2),
    ("step", Func::Step, 2, 2),
    ("hypot", Func::Hypot, 2, 2),
    ("clamp", Func::Clamp, 3, 3),
    ("mix", Func::Mix, 3, 3),
    ("lerp", Func::Mix, 3, 3),
    ("smoothstep", Func::Smoothstep, 3, 3),
    ("map", Func::Map, 5, 5),
    ("noise", Func::Noise, 1, 3),
    ("vec2", Func::Vec2, 1, 2),
    ("length", Func::Length, 1, 1),
    ("distance", Func::Distance, 2, 2),
    ("dist", Func::Distance, 2, 2),
    ("dot", Func::Dot, 2, 2),
    ("normalize", Func::Normalize, 1, 1),
    ("angle", Func::Angle, 1, 1),
    ("rgb", Func::Rgb, 3, 4),
    ("rgba", Func::Rgb, 4, 4),
    ("hsl", Func::Hsl, 3, 4),
];

#[derive(Clone, Debug)]
enum Expr {
    Num(f64),
    /// An input pin, by index.
    Var(usize),
    Builtin(Builtin),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// `cond ? then : else`.
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(Func, &'static str, Vec<Expr>),
    Member(Box<Expr>, Comp),
}

/// A parsed formula: its tree and its input pins (in order of first use).
#[derive(Clone, Debug)]
pub struct Program {
    expr: Expr,
    vars: Vec<(String, PortType)>,
    uses_time: bool,
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    depth: usize,
    vars: Vec<(String, PortType)>,
    uses_time: bool,
    /// Column just past the source, for "unexpected end" errors.
    end_col: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.tok)
    }

    fn peek_sym(&self, sym: &str) -> bool {
        matches!(self.peek(), Some(Tok::Sym(s)) if *s == sym)
    }

    fn col(&self) -> usize {
        self.toks.get(self.pos).map_or(self.end_col, |t| t.col)
    }

    /// "unexpected …" at the current token.
    fn unexpected(&self, wanted: &str) -> String {
        match self.peek() {
            None => format!("unexpected end of formula: expected {wanted}"),
            Some(t) => {
                let what = match t {
                    Tok::Num(n) => js::num_to_string(*n),
                    Tok::Ident(s) => s.clone(),
                    Tok::Sym(s) => (*s).to_owned(),
                };
                format!(
                    "unexpected \"{what}\" (column {}): expected {wanted}",
                    self.col()
                )
            }
        }
    }

    fn expect(&mut self, sym: &str) -> Result<(), String> {
        if self.peek_sym(sym) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.unexpected(&format!("\"{sym}\"")))
        }
    }

    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            Err("the formula nests too deeply".into())
        } else {
            Ok(())
        }
    }

    /// expr := binary(1) [ "?" expr ":" expr ]
    fn expr(&mut self) -> Result<Expr, String> {
        self.enter()?;
        let cond = self.binary(1)?;
        let out = if self.peek_sym("?") {
            self.pos += 1;
            let then = self.expr()?;
            self.expect(":")?;
            let other = self.expr()?;
            Expr::Cond(Box::new(cond), Box::new(then), Box::new(other))
        } else {
            cond
        };
        self.depth -= 1;
        Ok(out)
    }

    /// Precedence climbing over the left-associative infix operators.
    fn binary(&mut self, min_prec: u8) -> Result<Expr, String> {
        let mut lhs = self.unary()?;
        while let Some(Tok::Sym(s)) = self.peek() {
            let Some((op, prec)) = infix(s) else { break };
            if prec < min_prec {
                break;
            }
            self.pos += 1;
            let rhs = self.binary(prec + 1)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// unary := ("-" | "+" | "!") unary | power
    fn unary(&mut self) -> Result<Expr, String> {
        self.enter()?;
        let out = if self.peek_sym("-") {
            self.pos += 1;
            Expr::Neg(Box::new(self.unary()?))
        } else if self.peek_sym("+") {
            self.pos += 1;
            self.unary()?
        } else if self.peek_sym("!") {
            self.pos += 1;
            Expr::Not(Box::new(self.unary()?))
        } else {
            self.power()?
        };
        self.depth -= 1;
        Ok(out)
    }

    /// power := postfix [ "^" unary ]  (right-associative, above unary
    /// minus: `-x^2` is `-(x^2)`, `2^-1` is `0.5`).
    fn power(&mut self) -> Result<Expr, String> {
        let base = self.postfix()?;
        if self.peek_sym("^") {
            self.pos += 1;
            let exp = self.unary()?;
            return Ok(Expr::Binary(BinOp::Pow, Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }

    /// postfix := primary { "." component }
    fn postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.primary()?;
        while self.peek_sym(".") {
            self.pos += 1;
            let col = self.col();
            let comp = match self.peek() {
                Some(Tok::Ident(s)) => Comp::parse(s).ok_or_else(|| {
                    format!("unknown component \".{s}\" (column {col}): use .x .y or .r .g .b .a")
                })?,
                _ => return Err(self.unexpected("a component (.x .y .r .g .b .a)")),
            };
            self.pos += 1;
            if let Expr::Var(i) = e {
                // A variable read by component gets a vec2 / colour pin.
                let ty = &mut self.vars[i].1;
                if *ty == PortType::Number {
                    *ty = comp.port_type();
                }
            }
            e = Expr::Member(Box::new(e), comp);
        }
        Ok(e)
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let col = self.col();
        match self.peek().cloned() {
            Some(Tok::Num(n)) => {
                self.pos += 1;
                Ok(Expr::Num(n))
            }
            Some(Tok::Sym("(")) => {
                self.pos += 1;
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Some(Tok::Ident(name)) => {
                self.pos += 1;
                if self.peek_sym("(") {
                    return self.call(&name, col);
                }
                if let Some(b) = builtin(&name) {
                    if matches!(b, Builtin::Time | Builtin::Frame) {
                        self.uses_time = true;
                    }
                    return Ok(Expr::Builtin(b));
                }
                if let Some(v) = constant(&name) {
                    return Ok(Expr::Num(v));
                }
                let i = match self.vars.iter().position(|(n, _)| *n == name) {
                    Some(i) => i,
                    None => {
                        self.vars.push((name, PortType::Number));
                        self.vars.len() - 1
                    }
                };
                Ok(Expr::Var(i))
            }
            _ => Err(self.unexpected("a number, a name or \"(\"")),
        }
    }

    fn call(&mut self, name: &str, col: usize) -> Result<Expr, String> {
        let Some(&(fname, f, min, max)) = FUNCS.iter().find(|(n, ..)| *n == name) else {
            return Err(format!("unknown function \"{name}\" (column {col})"));
        };
        self.expect("(")?;
        let mut args = Vec::new();
        if !self.peek_sym(")") {
            loop {
                args.push(self.expr()?);
                if self.peek_sym(",") {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        self.expect(")")?;
        if args.len() < min || args.len() > max {
            let want = match (min, max) {
                (a, b) if a == b => format!("{a}"),
                (a, 16) => format!("{a} or more"),
                (a, b) => format!("{a} to {b}"),
            };
            let s = if want == "1" { "" } else { "s" };
            return Err(format!(
                "{fname}() takes {want} argument{s}, got {} (column {col})",
                args.len()
            ));
        }
        Ok(Expr::Call(f, fname, args))
    }
}

/// Parse a formula.
pub fn parse(src: &str) -> Result<Program, String> {
    if src.chars().count() > MAX_SOURCE {
        return Err(format!(
            "the formula is too long (over {MAX_SOURCE} characters)"
        ));
    }
    let toks = lex(src)?;
    if toks.is_empty() {
        return Err("the formula is empty".into());
    }
    let mut p = Parser {
        toks,
        pos: 0,
        depth: 0,
        vars: Vec::new(),
        uses_time: false,
        end_col: src.chars().count() + 1,
    };
    let expr = p.expr()?;
    if p.pos < p.toks.len() {
        return Err(p.unexpected("an operator or the end of the formula"));
    }
    Ok(Program {
        expr,
        vars: p.vars,
        uses_time: p.uses_time,
    })
}

/// The pins a formula needs, even when it doesn't parse yet (so wires
/// survive a typo): every name that isn't a function call, a component,
/// a built-in or a constant, typed by the components it is read with.
fn free_variables(src: &str) -> Vec<(String, PortType)> {
    if let Ok(p) = parse(src) {
        return p.vars;
    }
    let toks = lex_with(src, true).unwrap_or_default();
    let mut vars: Vec<(String, PortType)> = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        let Tok::Ident(name) = &t.tok else { continue };
        let prev_dot = i > 0 && toks[i - 1].tok == Tok::Sym(".");
        let next = toks.get(i + 1).map(|t| &t.tok);
        if prev_dot
            || next == Some(&Tok::Sym("("))
            || builtin(name).is_some()
            || constant(name).is_some()
        {
            continue;
        }
        let comp = match (next, toks.get(i + 2).map(|t| &t.tok)) {
            (Some(Tok::Sym(".")), Some(Tok::Ident(c))) => Comp::parse(c),
            _ => None,
        };
        match vars.iter_mut().find(|(n, _)| n == name) {
            Some((_, ty)) => {
                if let (PortType::Number, Some(c)) = (*ty, comp) {
                    *ty = c.port_type();
                }
            }
            None => vars.push((name.clone(), comp.map_or(PortType::Number, Comp::port_type))),
        }
    }
    vars
}

// ─── Evaluation ──────────────────────────────────────────────────────────────

/// A value inside the formula.
#[derive(Clone, Copy, Debug, PartialEq)]
enum V {
    N(f64),
    V2(f64, f64),
    /// r, g, b, a in 0..1.
    C([f64; 4]),
}

impl V {
    fn from_value(v: &Value) -> V {
        match v {
            Value::Num(n) => V::N(*n),
            Value::Vec2(p) => V::V2(p.x, p.y),
            Value::Color(c) => V::C([c.r, c.g, c.b, c.a]),
            Value::Object(_) => match kind_of(v) {
                Some(Kind::Vec2) => {
                    let p = super::shared::to_vec2(v);
                    V::V2(p.x, p.y)
                }
                Some(Kind::Color) => {
                    let c = super::shared::to_color(v);
                    V::C([c.r, c.g, c.b, c.a])
                }
                _ => V::N(v.as_number()),
            },
            other => V::N(other.as_number()),
        }
    }

    fn to_value(self) -> Value {
        match self {
            V::N(n) => Value::Num(n),
            V::V2(x, y) => Value::Vec2(Vec2 { x, y }),
            V::C([r, g, b, a]) => Value::Color(Color { r, g, b, a }),
        }
    }

    fn kind(self) -> &'static str {
        match self {
            V::N(_) => "a number",
            V::V2(..) => "a vec2",
            V::C(_) => "a colour",
        }
    }

    /// Apply `f` per component (a colour's alpha is kept).
    fn map(self, f: impl Fn(f64) -> f64) -> V {
        match self {
            V::N(n) => V::N(f(n)),
            V::V2(x, y) => V::V2(f(x), f(y)),
            V::C([r, g, b, a]) => V::C([f(r), f(g), f(b), a]),
        }
    }

    fn num(self, what: &str) -> Result<f64, String> {
        match self {
            V::N(n) => Ok(n),
            other => Err(format!("{what} needs a number, got {}", other.kind())),
        }
    }
}

/// Combine two values per component; a number is broadcast over a vec2's
/// components or a colour's r, g, b (the colour keeps its alpha).
fn zip(a: V, b: V, what: &str, f: impl Fn(f64, f64) -> f64) -> Result<V, String> {
    Ok(match (a, b) {
        (V::N(x), V::N(y)) => V::N(f(x, y)),
        (V::V2(ax, ay), V::V2(bx, by)) => V::V2(f(ax, bx), f(ay, by)),
        (V::V2(x, y), V::N(n)) => V::V2(f(x, n), f(y, n)),
        (V::N(n), V::V2(x, y)) => V::V2(f(n, x), f(n, y)),
        (V::C(p), V::C(q)) => V::C([f(p[0], q[0]), f(p[1], q[1]), f(p[2], q[2]), f(p[3], q[3])]),
        (V::C([r, g, bb, al]), V::N(n)) => V::C([f(r, n), f(g, n), f(bb, n), al]),
        (V::N(n), V::C([r, g, bb, al])) => V::C([f(n, r), f(n, g), f(n, bb), al]),
        (x, y) => {
            return Err(format!(
                "{what} can't combine {} and {}",
                x.kind(),
                y.kind()
            ))
        }
    })
}

fn truthy(v: V, what: &str) -> Result<bool, String> {
    let n = v.num(what)?;
    Ok(n != 0.0 && !n.is_nan())
}

fn flag(b: bool) -> V {
    V::N(f64::from(u8::from(b)))
}

/// Floored modulo: wraps negatives into [0, b).
fn floor_mod(a: f64, b: f64) -> f64 {
    a - b * (a / b).floor()
}

/// What the formula sees besides its pins.
struct Env<'a> {
    vars: &'a [V],
    t: f64,
    frame: f64,
    index: f64,
    count: f64,
}

fn binary(op: BinOp, a: V, b: V) -> Result<V, String> {
    let cmp = |f: fn(f64, f64) -> bool, s: &str| -> Result<V, String> {
        Ok(flag(f(a.num(s)?, b.num(s)?)))
    };
    match op {
        BinOp::Add => zip(a, b, "+", |x, y| x + y),
        BinOp::Sub => zip(a, b, "-", |x, y| x - y),
        BinOp::Mul => zip(a, b, "*", |x, y| x * y),
        BinOp::Div => zip(a, b, "/", |x, y| x / y),
        BinOp::Mod => zip(a, b, "%", floor_mod),
        BinOp::Pow => zip(a, b, "^", f64::powf),
        BinOp::Eq => cmp(|x, y| x == y, "=="),
        BinOp::Ne => cmp(|x, y| x != y, "!="),
        BinOp::Lt => cmp(|x, y| x < y, "<"),
        BinOp::Le => cmp(|x, y| x <= y, "<="),
        BinOp::Gt => cmp(|x, y| x > y, ">"),
        BinOp::Ge => cmp(|x, y| x >= y, ">="),
        // Short-circuiting is done in `eval`.
        BinOp::And | BinOp::Or => unreachable!(),
    }
}

fn eval(e: &Expr, env: &Env) -> Result<V, String> {
    Ok(match e {
        Expr::Num(n) => V::N(*n),
        Expr::Var(i) => env.vars[*i],
        Expr::Builtin(b) => V::N(match b {
            Builtin::Time => env.t,
            Builtin::Frame => env.frame,
            Builtin::Index => env.index,
            Builtin::Count => env.count,
        }),
        Expr::Neg(x) => eval(x, env)?.map(|v| -v),
        Expr::Not(x) => flag(!truthy(eval(x, env)?, "!")?),
        Expr::Binary(BinOp::And, a, b) => {
            flag(truthy(eval(a, env)?, "&&")? && truthy(eval(b, env)?, "&&")?)
        }
        Expr::Binary(BinOp::Or, a, b) => {
            flag(truthy(eval(a, env)?, "||")? || truthy(eval(b, env)?, "||")?)
        }
        Expr::Binary(op, a, b) => binary(*op, eval(a, env)?, eval(b, env)?)?,
        Expr::Cond(c, a, b) => {
            if truthy(eval(c, env)?, "the condition before ?")? {
                eval(a, env)?
            } else {
                eval(b, env)?
            }
        }
        Expr::Member(x, comp) => {
            let v = eval(x, env)?;
            match (v, comp) {
                (V::V2(x, _), Comp::X) => V::N(x),
                (V::V2(_, y), Comp::Y) => V::N(y),
                (V::C(c), Comp::R) => V::N(c[0]),
                (V::C(c), Comp::G) => V::N(c[1]),
                (V::C(c), Comp::B) => V::N(c[2]),
                (V::C(c), Comp::A) => V::N(c[3]),
                (v, comp) => {
                    let needs = match comp {
                        Comp::X | Comp::Y => "a vec2",
                        _ => "a colour",
                    };
                    return Err(format!(".{} needs {needs}, got {}", comp.name(), v.kind()));
                }
            }
        }
        Expr::Call(f, name, args) => {
            let mut vals = [V::N(0.0); 16];
            for (slot, a) in vals.iter_mut().zip(args) {
                *slot = eval(a, env)?;
            }
            call(*f, name, &vals[..args.len()])?
        }
    })
}

fn call(f: Func, name: &str, a: &[V]) -> Result<V, String> {
    let unary = |g: fn(f64) -> f64| Ok(a[0].map(g));
    let what = format!("{name}()");
    match f {
        Func::Sin => unary(f64::sin),
        Func::Cos => unary(f64::cos),
        Func::Tan => unary(f64::tan),
        Func::Asin => unary(f64::asin),
        Func::Acos => unary(f64::acos),
        Func::Atan => unary(f64::atan),
        Func::Sinh => unary(f64::sinh),
        Func::Cosh => unary(f64::cosh),
        Func::Tanh => unary(f64::tanh),
        Func::Exp => unary(f64::exp),
        Func::Log => unary(f64::ln),
        Func::Log2 => unary(f64::log2),
        Func::Log10 => unary(f64::log10),
        Func::Sqrt => unary(f64::sqrt),
        Func::Abs => unary(f64::abs),
        Func::Floor => unary(f64::floor),
        Func::Ceil => unary(f64::ceil),
        Func::Round => unary(js::round),
        Func::Fract => unary(|x| x - x.floor()),
        Func::Sign => unary(js::sign),
        Func::Trunc => unary(f64::trunc),
        Func::Radians => unary(f64::to_radians),
        Func::Degrees => unary(f64::to_degrees),
        Func::Atan2 => Ok(V::N(a[0].num(&what)?.atan2(a[1].num(&what)?))),
        Func::Pow => zip(a[0], a[1], &what, f64::powf),
        Func::Min => a[1..]
            .iter()
            .try_fold(a[0], |acc, &v| zip(acc, v, &what, js::min)),
        Func::Max => a[1..]
            .iter()
            .try_fold(a[0], |acc, &v| zip(acc, v, &what, js::max)),
        Func::Mod => zip(a[0], a[1], &what, floor_mod),
        Func::Step => zip(a[0], a[1], &what, |edge, x| f64::from(u8::from(x >= edge))),
        Func::Hypot => Ok(V::N(js::hypot(a[0].num(&what)?, a[1].num(&what)?))),
        Func::Clamp => {
            let lo = zip(a[0], a[1], &what, js::max)?;
            zip(lo, a[2], &what, js::min)
        }
        Func::Mix => {
            // a + (b − a)·t
            let d = zip(a[1], a[0], &what, |b, a| b - a)?;
            let d = zip(d, a[2], &what, |d, t| d * t)?;
            zip(a[0], d, &what, |a, d| a + d)
        }
        Func::Smoothstep => {
            let span = zip(a[1], a[0], &what, |e1, e0| e1 - e0)?;
            let x = zip(a[2], a[0], &what, |x, e0| x - e0)?;
            let t = zip(x, span, &what, |x, s| (x / s).clamp(0.0, 1.0))?;
            Ok(t.map(|t| t * t * (3.0 - 2.0 * t)))
        }
        Func::Map => {
            // c + (x − a) / (b − a) · (d − c)
            let x = zip(a[0], a[1], &what, |x, lo| x - lo)?;
            let span = zip(a[2], a[1], &what, |hi, lo| hi - lo)?;
            let f = zip(x, span, &what, |x, s| x / s)?;
            let out = zip(a[4], a[3], &what, |hi, lo| hi - lo)?;
            let scaled = zip(f, out, &what, |f, o| f * o)?;
            zip(a[3], scaled, &what, |lo, s| lo + s)
        }
        Func::Noise => {
            let n = |i: usize| a.get(i).map_or(Ok(0.0), |v| v.num(&what));
            Ok(V::N(perlin_2d(n(0)?, n(1)?, node_seed(n(2)?))))
        }
        Func::Vec2 => {
            let x = a[0].num(&what)?;
            let y = a.get(1).map_or(Ok(x), |v| v.num(&what))?;
            Ok(V::V2(x, y))
        }
        Func::Length => Ok(V::N(length(a[0]))),
        Func::Distance => Ok(V::N(length(zip(a[0], a[1], &what, |x, y| x - y)?))),
        Func::Dot => match (a[0], a[1]) {
            (V::V2(ax, ay), V::V2(bx, by)) => Ok(V::N(ax * bx + ay * by)),
            (V::N(x), V::N(y)) => Ok(V::N(x * y)),
            (x, y) => Err(format!(
                "{what} needs two vec2s, got {} and {}",
                x.kind(),
                y.kind()
            )),
        },
        Func::Normalize => match a[0] {
            V::V2(x, y) => {
                let l = js::hypot(x, y);
                Ok(if l > 0.0 {
                    V::V2(x / l, y / l)
                } else {
                    V::V2(0.0, 0.0)
                })
            }
            other => Err(format!("{what} needs a vec2, got {}", other.kind())),
        },
        Func::Angle => match a[0] {
            V::V2(x, y) => Ok(V::N(y.atan2(x))),
            other => Err(format!("{what} needs a vec2, got {}", other.kind())),
        },
        Func::Rgb => {
            let alpha = a.get(3).map_or(Ok(1.0), |v| v.num(&what))?;
            Ok(V::C([
                a[0].num(&what)?,
                a[1].num(&what)?,
                a[2].num(&what)?,
                alpha,
            ]))
        }
        Func::Hsl => {
            let alpha = a.get(3).map_or(Ok(1.0), |v| v.num(&what))?;
            let c = hsl_to_color01(
                a[0].num(&what)? * 360.0,
                a[1].num(&what)? * 100.0,
                a[2].num(&what)? * 100.0,
                alpha,
            );
            Ok(V::C([c.r, c.g, c.b, c.a]))
        }
    }
}

fn length(v: V) -> f64 {
    match v {
        V::N(n) => n.abs(),
        V::V2(x, y) => js::hypot(x, y),
        V::C([r, g, b, _]) => (r * r + g * g + b * b).sqrt(),
    }
}

// ─── Node ────────────────────────────────────────────────────────────────────

fn source(params: &Params) -> &str {
    params
        .get("expr")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(DEFAULT_EXPR)
}

/// `dynamicInputs`: one spread-kind pin per free variable.
fn expression_inputs(params: &Params) -> Vec<PortSpec> {
    free_variables(source(params))
        .into_iter()
        .map(|(name, ty)| {
            let default = match ty {
                PortType::Vec2 => Value::Vec2(Vec2::default()),
                PortType::Color => Value::Color(Color::WHITE),
                _ => Value::Num(0.0),
            };
            PortSpec::new(name, ty)
                .kind(PortKind::Spread)
                .default(default)
        })
        .collect()
}

struct Expression {
    src: String,
    program: Result<Program, String>,
}

impl Expression {
    fn compile(src: &str) -> Expression {
        Expression {
            src: src.to_owned(),
            program: parse(src),
        }
    }
}

fn expression(init: &NodeInit) -> Box<dyn NodeImpl> {
    Box::new(Expression::compile(source(init.params)))
}

impl NodeImpl for Expression {
    fn eval(&mut self, inputs: &[Spread], cx: &mut EvalCtx) -> NodeResult {
        let src = cx
            .param("expr")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(DEFAULT_EXPR);
        if src != self.src {
            *self = Expression::compile(src);
        }
        let p = self.program.as_ref().map_err(Clone::clone)?;
        let pins = inputs
            .get(..p.vars.len())
            .ok_or_else(|| "the formula's inputs changed: recompile the patch".to_owned())?;
        let count = if pins.is_empty() { 1 } else { cyclic_len(pins) };
        let mut vars = vec![V::N(0.0); pins.len()];
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            for (slot, pin) in vars.iter_mut().zip(pins) {
                *slot = V::from_value(&cyclic(pin, i));
            }
            let env = Env {
                vars: &vars,
                t: cx.frame.t,
                frame: cx.frame.frame_count,
                index: i as f64,
                count: count as f64,
            };
            out.push(eval(&p.expr, &env)?.to_value());
        }
        Ok(vec![Arc::from(out)])
    }

    /// A formula that reads the clock runs every tick.
    fn memoise(&self) -> bool {
        !self.program.as_ref().is_ok_and(|p| p.uses_time)
    }
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::{n, node, num, Harness};
    use super::*;
    use serde_json::json;

    const ENV: Env<'static> = Env {
        vars: &[],
        t: 0.0,
        frame: 0.0,
        index: 0.0,
        count: 1.0,
    };

    fn value(src: &str) -> Result<V, String> {
        eval(&parse(src)?.expr, &ENV)
    }

    /// Evaluate a formula with no pins to a number.
    fn calc(src: &str) -> f64 {
        match value(src) {
            Ok(V::N(x)) => x,
            other => panic!("{src} gave {other:?}"),
        }
    }

    fn err(src: &str) -> String {
        parse(src).err().unwrap_or_else(|| panic!("{src} parsed"))
    }

    fn expr_node(src: &str) -> Harness {
        node("math/expression", json!({ "expr": src }))
    }

    fn nums(v: &[f64]) -> Vec<Value> {
        v.iter().map(|x| n(*x)).collect()
    }

    #[test]
    fn precedence_and_associativity() {
        for (src, want) in [
            ("1 + 2 * 3", 7.0),
            ("(1 + 2) * 3", 9.0),
            ("10 - 4 - 3", 3.0),
            ("8 / 4 / 2", 1.0),
            ("2 * 3 % 4", 2.0),
            ("-2 ^ 2", -4.0),
            ("2 ^ 3 ^ 2", 512.0),
            ("2 ^ -1", 0.5),
            ("-(2 + 3)", -5.0),
            ("+4", 4.0),
            ("--4", 4.0),
            ("1 + 2 < 4", 1.0),
            ("1 < 2 == 1", 1.0),
            ("1 < 2 && 3 < 2 || 1", 1.0),
            ("0 || 0 && 1", 0.0),
            ("!0 + 1", 2.0),
            ("1 ? 2 : 3", 2.0),
            ("0 ? 2 : 1 ? 4 : 5", 4.0),
            ("1 + 1 ? 10 : 20", 10.0),
            ("-7 % 3", 2.0),
            ("1e3 + .5 + 2.5E-1", 1000.75),
            ("pi", std::f64::consts::PI),
            ("tau / 2 - pi", 0.0),
        ] {
            assert_eq!(calc(src), want, "{src}");
        }
    }

    #[test]
    fn functions() {
        let close = |src: &str, want: f64| {
            let got = calc(src);
            assert!((got - want).abs() < 1e-12, "{src}: {got} vs {want}");
        };
        close("sin(pi / 2) + cos(0)", 2.0);
        close("atan2(1, 1)", std::f64::consts::FRAC_PI_4);
        close("min(3, 1, 2) + max(4, 9)", 10.0);
        close("clamp(5, 0, 1) + clamp(-5, 0, 1)", 1.0);
        close("mix(10, 20, 0.25)", 12.5);
        close("lerp(10, 20, 0.5)", 15.0);
        close("smoothstep(0, 1, 0.5)", 0.5);
        close("map(5, 0, 10, 100, 200)", 150.0);
        close("fract(-0.25) + floor(1.5) + ceil(1.2) + round(2.5)", 6.75);
        close("mod(-1, 4) + step(0.5, 0.7) + sign(-3)", 3.0);
        close("sqrt(16) + abs(-2) + pow(2, 3) + hypot(3, 4)", 19.0);
        close("degrees(pi) + radians(180)", 180.0 + std::f64::consts::PI);
        close(
            "length(vec2(3, 4)) + distance(vec2(1, 1), vec2(4, 5))",
            10.0,
        );
        close(
            "dot(vec2(1, 2), vec2(3, 4)) + angle(vec2(0, 1))",
            11.0 + std::f64::consts::FRAC_PI_2,
        );
        close("normalize(vec2(0, 5)).y + vec2(7).x", 8.0);
        close("rgb(0.1, 0.2, 0.3).g + rgba(0, 0, 0, 0.5).a", 0.7);
        close("hsl(0, 1, 0.5).r + hsl(1/3, 1, 0.5).g", 2.0);
        close("noise(0.5, 0.5)", perlin_2d(0.5, 0.5, 0.0));
        close("noise(2.5, 3.5, 7)", perlin_2d(2.5, 3.5, 7.0));
    }

    #[test]
    fn vectors_and_colours_work_per_component() {
        assert_eq!(
            value("vec2(1, 2) * 3 + vec2(10, 20)"),
            Ok(V::V2(13.0, 26.0))
        );
        assert_eq!(
            value("rgb(0.2, 0.4, 0.5, 0.5) * 2"),
            Ok(V::C([0.4, 0.8, 1.0, 0.5]))
        );
        assert_eq!(
            value("mix(rgb(0, 0, 0), rgb(1, 1, 1), 0.5)"),
            Ok(V::C([0.5, 0.5, 0.5, 1.0]))
        );
        assert_eq!(
            value("vec2(1, 2) + rgb(1, 1, 1)"),
            Err("+ can't combine a vec2 and a colour".into())
        );
        assert_eq!(
            value("vec2(1, 2).r"),
            Err(".r needs a colour, got a vec2".into())
        );
        assert_eq!(
            value("vec2(1, 2) < 3"),
            Err("< needs a number, got a vec2".into())
        );
    }

    #[test]
    fn variables_become_typed_pins() {
        let p = parse("sin(x * 3) + y * t").unwrap();
        let names: Vec<_> = p.vars.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["x", "y"]);
        assert!(p.uses_time);
        let p = parse("p.x + c.r * k + p.y + index / count + pi").unwrap();
        assert_eq!(
            p.vars,
            [
                ("p".into(), PortType::Vec2),
                ("c".into(), PortType::Color),
                ("k".into(), PortType::Number),
            ]
        );
        assert!(!p.uses_time);
        assert!(parse("frame * 2").unwrap().uses_time);
        // A typo keeps the pins (and so the wires) it can still see.
        for (src, want) in [
            ("sin(x * 3) + y *", ["x", "y"]),
            ("x $ y", ["x", "y"]),
            ("(x + y", ["x", "y"]),
            ("foo(x, p.y)", ["x", "p"]),
        ] {
            let vars = free_variables(src);
            let names: Vec<_> = vars.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names, want, "{src}");
        }
        assert_eq!(free_variables("foo(x, p.y)")[1].1, PortType::Vec2);
        let ports = expression_inputs(json!({"expr": "a.x * b"}).as_object().unwrap());
        assert_eq!(ports.len(), 2);
        assert_eq!(
            (ports[0].ty, ports[0].kind),
            (PortType::Vec2, PortKind::Spread)
        );
        assert_eq!(ports[1].ty, PortType::Number);
        // The catalogue's static pins are the default formula's.
        let cat = crate::spec::Catalogue::builtin();
        let spec = cat.get("math/expression").unwrap();
        let statics: Vec<_> = spec.inputs.iter().map(|p| p.name.clone()).collect();
        let dynamic: Vec<_> = expression_inputs(&spec.default_params)
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(statics, dynamic);
    }

    #[test]
    fn clear_errors() {
        assert_eq!(err("foo(1)"), "unknown function \"foo\" (column 1)");
        assert_eq!(
            err("1 + sin(1, 2)"),
            "sin() takes 1 argument, got 2 (column 5)"
        );
        assert_eq!(
            err("clamp(1)"),
            "clamp() takes 3 arguments, got 1 (column 1)"
        );
        assert_eq!(
            err("min(1)"),
            "min() takes 2 or more arguments, got 1 (column 1)"
        );
        assert_eq!(
            err("noise()"),
            "noise() takes 1 to 3 arguments, got 0 (column 1)"
        );
        assert_eq!(
            err("1 +"),
            "unexpected end of formula: expected a number, a name or \"(\""
        );
        assert_eq!(err("(1 + 2"), "unexpected end of formula: expected \")\"");
        assert_eq!(
            err("1 2"),
            "unexpected \"2\" (column 3): expected an operator or the end of the formula"
        );
        assert_eq!(
            err("2 * )"),
            "unexpected \")\" (column 5): expected a number, a name or \"(\""
        );
        assert_eq!(err("x = 1"), "unexpected \"=\" (column 3)");
        assert_eq!(err("a $ b"), "unexpected \"$\" (column 3)");
        assert_eq!(
            err("p.z"),
            "unknown component \".z\" (column 3): use .x .y or .r .g .b .a"
        );
        assert_eq!(err("1.2.3"), "\"1.2.3\" isn't a number (column 1)");
        assert_eq!(err("   "), "the formula is empty");
        assert_eq!(err(&"(".repeat(200)), "the formula nests too deeply");
        assert_eq!(err(&"-".repeat(200)), "the formula nests too deeply");
        assert!(err(&"1+".repeat(3000)).starts_with("the formula is too long"));
        // There are no loops or statements to call.
        assert!(err("while(1)").starts_with("unknown function \"while\""));
    }

    #[test]
    fn runs_per_element_over_spreads() {
        let mut h = expr_node("x * 2 + y");
        let out = h.eval(vec![nums(&[1.0, 2.0, 3.0]), nums(&[10.0])]).unwrap();
        assert_eq!(
            out[0].iter().map(num).collect::<Vec<_>>(),
            [12.0, 14.0, 16.0]
        );
        // Shorter spreads cycle; an empty one empties the result.
        let out = h
            .eval(vec![nums(&[1.0, 2.0, 3.0, 4.0]), nums(&[0.0, 100.0])])
            .unwrap();
        assert_eq!(
            out[0].iter().map(num).collect::<Vec<_>>(),
            [2.0, 104.0, 6.0, 108.0]
        );
        assert!(h.eval(vec![nums(&[1.0]), Vec::new()]).unwrap()[0].is_empty());
        // `index` / `count` see the element.
        let mut h = expr_node("a + index / count");
        let out = h.eval(vec![nums(&[0.0, 0.0, 0.0, 0.0])]).unwrap();
        assert_eq!(
            out[0].iter().map(num).collect::<Vec<_>>(),
            [0.0, 0.25, 0.5, 0.75]
        );
        // No pins: one value.
        let mut h = expr_node("2 + 2");
        assert_eq!(h.eval(vec![]).unwrap()[0].len(), 1);
    }

    #[test]
    fn vec2_and_colour_pins() {
        let mut h = expr_node("vec2(p.x * k, p.y + 1)");
        let p = |x, y| Value::Vec2(Vec2 { x, y });
        let out = h
            .eval(vec![vec![p(1.0, 2.0), p(3.0, 4.0)], nums(&[10.0])])
            .unwrap();
        assert!(matches!(out[0][0], Value::Vec2(v) if v == Vec2 { x: 10.0, y: 3.0 }));
        assert!(matches!(out[0][1], Value::Vec2(v) if v == Vec2 { x: 30.0, y: 5.0 }));
        let mut h = expr_node("rgb(c.g, c.r, c.b, c.a)");
        let c = Value::Color(Color {
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 0.4,
        });
        let out = h.eval(vec![vec![c]]).unwrap();
        assert!(
            matches!(out[0][0], Value::Color(c) if (c.r, c.g, c.b, c.a) == (0.2, 0.1, 0.3, 0.4))
        );
        // A number where a component is read reports on the node.
        let e = h.eval(vec![nums(&[1.0])]).unwrap_err();
        assert_eq!(e, ".g needs a colour, got a number");
    }

    #[test]
    fn time_comes_from_the_frame_and_disables_the_memo() {
        let mut h = expr_node("t * 2 + frame");
        h.frame.t = 1.5;
        h.frame.frame_count = 10.0;
        assert_eq!(num(&h.eval(vec![]).unwrap()[0][0]), 13.0);
        assert!(!Expression::compile("t * 2").memoise());
        assert!(Expression::compile("x * 2").memoise());
        // A formula that doesn't parse reports its error on the node.
        let mut h = expr_node("sin(");
        assert_eq!(
            h.eval(vec![]).unwrap_err(),
            "unexpected end of formula: expected a number, a name or \"(\""
        );
    }
}
