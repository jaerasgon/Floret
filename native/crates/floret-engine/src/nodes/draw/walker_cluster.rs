//! `draw/walker-cluster`. Port of `src/nodes/draw/walker-cluster.ts`.
//!
//! Spawns random walkers on two anchor grids inside a fixed `WORLD × WORLD`
//! arena, replays each for `steps` jumps (line trails or concentric circle
//! pairs, decaying by 0.95 per step), and bakes the result into a clipped
//! group scaled into a `size × size` cell centred on `pos`.

use crate::js::{self, Mulberry32};
use crate::nodes::shared::{first, to_color, to_vec2};
use crate::scene::{
    default_style, scale_mat, translate_mat, Paint, RectF, ResolvedStyle, SceneNode,
};
use crate::value::{one, Color, Spread, Value};
use std::sync::Arc;

const WORLD: f64 = 250.0;
const COARSE_ANCHOR_STEP: f64 = 50.0;
const FINE_ANCHOR_STEP: f64 = 25.0;
const ATTEMPTS_PER_ANCHOR: usize = 2;
const BASE_SPAWN_PROB: f64 = 0.25;
const INITIAL_WEIGHT_MAX: f64 = 100.0;
const DECAY: f64 = 0.95;
const WEIGHT_OVERLAY_THRESHOLD: f64 = 10.0;

const fn rgb(r: f64, g: f64, b: f64) -> Color {
    Color { r, g, b, a: 1.0 }
}

/// The source sketch's palette, used when nothing is wired into `colors`.
/// (0.318 is a colour channel, not 1/π.)
#[allow(clippy::approx_constant)]
const DEFAULT_PALETTE: [Color; 14] = [
    rgb(0.949, 0.922, 0.541),
    rgb(0.996, 0.816, 0.000),
    rgb(0.988, 0.518, 0.020),
    rgb(0.929, 0.212, 0.102),
    rgb(0.886, 0.941, 0.953),
    rgb(0.702, 0.863, 0.878),
    rgb(0.267, 0.392, 0.631),
    rgb(0.125, 0.188, 0.318),
    rgb(1.000, 0.773, 0.780),
    rgb(0.953, 0.596, 0.765),
    rgb(0.812, 0.220, 0.584),
    rgb(0.427, 0.208, 0.541),
    rgb(0.024, 0.706, 0.690),
    rgb(0.294, 0.541, 0.373),
];

/// Box–Muller: one standard-normal sample.
fn gaussian(rng: &mut Mulberry32) -> f64 {
    let mut u = 0.0;
    let mut v = 0.0;
    while u == 0.0 {
        u = rng.next_f64();
    }
    while v == 0.0 {
        v = rng.next_f64();
    }
    (-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * v).cos()
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn pick_jittered_color(rng: &mut Mulberry32, palette: &[Color]) -> Color {
    if palette.is_empty() {
        return Color::WHITE;
    }
    let base = palette[(rng.next_f64() * palette.len() as f64).floor() as usize];
    let j = 0.012;
    let r = clamp01(base.r + gaussian(rng) * j);
    let g = clamp01(base.g + gaussian(rng) * j);
    let b = clamp01(base.b + gaussian(rng) * j);
    Color { r, g, b, a: 1.0 }
}

/// The node's own `colorToCss`: no clamping, always `rgb()`.
fn color_to_css(c: Color) -> Paint {
    let ch = |v: f64| js::num_to_string(js::round(v * 255.0));
    Paint::css(&format!("rgb({},{},{})", ch(c.r), ch(c.g), ch(c.b)))
}

/// `typeof v === 'number' ? v : dflt` on slot 0.
fn num(inputs: &[Spread], i: usize, dflt: f64) -> f64 {
    first(inputs, i).as_f64().unwrap_or(dflt)
}

struct Walker {
    x: f64,
    y: f64,
    step: f64,
    w: f64,
    rad: f64,
    shape: u8,
    c1: Paint,
    c2: Paint,
}

pub fn compute(inputs: &[Spread]) -> Vec<Spread> {
    let raw_colors: &[Value] = inputs.first().map_or(&[], |s| &s[..]);
    let palette: Vec<Color> = if raw_colors.is_empty() {
        DEFAULT_PALETTE.to_vec()
    } else {
        raw_colors.iter().map(to_color).collect()
    };
    let pos = to_vec2(&first(inputs, 1));
    let size = js::max(1.0, num(inputs, 2, 140.0));
    let density = js::max(0.0, js::min(2.0, num(inputs, 3, 1.0)));
    let steps = js::max(1.0, num(inputs, 4, 200.0).floor());
    let seed = js::to_uint32(num(inputs, 5, 0.0).floor());

    let mut rng = Mulberry32::new(seed);
    let spawn_prob = BASE_SPAWN_PROB * density;
    let render_scale = size / WORLD;

    let mut walkers: Vec<Walker> = Vec::new();
    let mut spawn = |rng: &mut Mulberry32, x: f64, y: f64, step: f64| {
        let w = rng.next_f64() * INITIAL_WEIGHT_MAX;
        let shape = u8::from(rng.next_f64() >= 0.5);
        let c1 = color_to_css(pick_jittered_color(rng, &palette));
        let c2 = color_to_css(pick_jittered_color(rng, &palette));
        walkers.push(Walker {
            x,
            y,
            step,
            w,
            rad: step / 3.0,
            shape,
            c1,
            c2,
        });
    };

    let grid = |step: f64| {
        let mut out = Vec::new();
        let mut ax = 0.0;
        while ax <= WORLD {
            let mut ay = 0.0;
            while ay <= WORLD {
                out.push((ax, ay));
                ay += step;
            }
            ax += step;
        }
        out
    };
    for (ax, ay) in grid(COARSE_ANCHOR_STEP) {
        for _ in 0..ATTEMPTS_PER_ANCHOR {
            if rng.next_f64() < spawn_prob {
                spawn(&mut rng, ax, ay, WORLD * 0.025);
            }
        }
    }
    for (ax, ay) in grid(FINE_ANCHOR_STEP) {
        for _ in 0..ATTEMPTS_PER_ANCHOR {
            if rng.next_f64() < spawn_prob {
                let r = rng.next_f64();
                let step = if r < 1.0 / 3.0 {
                    WORLD * 0.05
                } else if r < 2.0 / 3.0 {
                    WORLD * 0.1
                } else {
                    WORLD * 0.2
                };
                spawn(&mut rng, ax, ay, step);
            }
        }
    }

    // Replay every walker and bake its steps straight into scene nodes
    // (TS records commands first; the PRNG order is the same).
    let style = |fill: Option<&Paint>, stroke: Option<&Paint>, weight: f64| {
        Arc::new(ResolvedStyle {
            fill: fill.cloned(),
            stroke: stroke.cloned(),
            stroke_weight: weight,
            ..(**default_style()).clone()
        })
    };
    let mut children: Vec<Arc<SceneNode>> = Vec::new();
    for wlk in &mut walkers {
        let base_weight = default_style().stroke_weight;
        let primary_fill = style(Some(&wlk.c2), None, base_weight);
        let secondary_fill = style(Some(&wlk.c1), None, base_weight);
        let mut it = 0.0;
        while it < steps {
            it += 1.0;
            let dx = (rng.next_f64() * 3.0).floor() - 1.0;
            let dy = (rng.next_f64() * 3.0).floor() - 1.0;
            let (ox, oy) = (wlk.x, wlk.y);
            wlk.x += wlk.step * dx;
            wlk.y += wlk.step * dy;
            wlk.w *= DECAY;
            wlk.rad *= DECAY;
            if wlk.shape == 0 {
                let size = wlk.w;
                let line = |st| {
                    Arc::new(SceneNode::Line {
                        x1: ox,
                        y1: oy,
                        x2: wlk.x,
                        y2: wlk.y,
                        style: st,
                    })
                };
                children.push(line(style(None, Some(&wlk.c2), size)));
                if size > WEIGHT_OVERLAY_THRESHOLD {
                    children.push(line(style(None, Some(&wlk.c1), size / 2.0)));
                }
            } else {
                let size = wlk.rad;
                let ellipse = |d: f64, st: &Arc<ResolvedStyle>| {
                    Arc::new(SceneNode::Ellipse {
                        x: wlk.x,
                        y: wlk.y,
                        w: d,
                        h: d,
                        mode: Arc::from("center"),
                        style: st.clone(),
                    })
                };
                children.push(ellipse(size, &primary_fill));
                children.push(ellipse(size / 2.0, &secondary_fill));
            }
        }
    }

    let scaled = SceneNode::Group {
        children: Arc::from(children),
        transform: None,
        blend_mode: None,
        clip: Some(RectF {
            x: 0.0,
            y: 0.0,
            w: WORLD,
            h: WORLD,
        }),
    };
    let inner = SceneNode::Group {
        children: Arc::from([Arc::new(scaled)]),
        transform: Some(scale_mat(render_scale, render_scale)),
        blend_mode: None,
        clip: None,
    };
    let node = SceneNode::Group {
        children: Arc::from([Arc::new(inner)]),
        transform: Some(translate_mat(pos.x - size / 2.0, pos.y - size / 2.0)),
        blend_mode: None,
        clip: None,
    };
    vec![one(Value::Scene(Arc::new(node)))]
}
