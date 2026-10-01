//! `force/*` nodes. Ported from `src/nodes/force/` (incl. `_shared.ts`).
//!
//! Each node outputs a [`Force`] handle: the TS closure's captured
//! constants as data, applied by `Force::apply`. Nodes are memoised like
//! the TS runtime does (see `particles::memo`), so an unchanged node keeps
//! handing out the same force — which matters for turbulence / wind, whose
//! closure clock advances every time an emitter applies them.

use super::particles::{handle, memo, num_or, vec2_or};
use super::shared::first;
use super::Registrar;
use crate::js;
use crate::node::{raw, scalar, NodeImpl, NodeInit};
use crate::particles::{js_hypot, FalloffCurve, Force, ForceKind, Handle};
use crate::scene::{default_style, Paint, ResolvedStyle, SceneNode};
use crate::value::{one, Spread, Value, Vec2};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("force/drag", drag);
    r.node("force/gravity", gravity);
    r.node("force/attractor", attractor);
    r.node("force/repulsor", repulsor);
    r.node("force/vortex", vortex);
    r.node("force/turbulence", turbulence);
    r.node("force/wind", wind);
}

fn force(kind: ForceKind) -> Value {
    Value::Handle(Handle::Force(Arc::new(Force::new(kind))))
}

fn force_spread(kind: ForceKind) -> Spread {
    handle(Handle::Force(Arc::new(Force::new(kind))))
}

/// `ringViz`: a translucent ring showing a force's region.
fn ring_viz(cx: f64, cy: f64, radius: f64, color: &str) -> Value {
    let style = ResolvedStyle {
        fill: None,
        stroke: Some(Paint::css(color)),
        stroke_weight: 1.5,
        ..(**default_style()).clone()
    };
    Value::Scene(Arc::new(SceneNode::Ellipse {
        x: cx,
        y: cy,
        w: radius * 2.0,
        h: radius * 2.0,
        mode: Arc::from("center"),
        style: Arc::new(style),
    }))
}

/// The `viz` output: the ring when `showViz`, else an empty group.
fn viz(show: bool, cx: f64, cy: f64, r: f64, color: &str) -> Spread {
    one(if show {
        ring_viz(cx, cy, r, color)
    } else {
        super::particles::empty_group()
    })
}

/// Inputs shared by attractor / repulsor: center, strength, radius,
/// falloff, showViz.
struct Radial {
    center: Vec2,
    strength: f64,
    r: f64,
    curve: FalloffCurve,
    show_viz: bool,
}

fn radial(inputs: &[Spread]) -> Radial {
    Radial {
        center: vec2_or(&first(inputs, 0), Vec2 { x: 0.0, y: 0.0 }),
        strength: num_or(&first(inputs, 1), 200.0),
        r: js::max(0.01, num_or(&first(inputs, 2), 200.0)),
        curve: FalloffCurve::coerce(&first(inputs, 3)),
        show_viz: first(inputs, 4).truthy(),
    }
}

/// `force/drag`: `f -= v · strength`.
fn drag(init: &NodeInit) -> Box<dyn NodeImpl> {
    memo(scalar(init, |args| {
        Ok(vec![force(ForceKind::Drag {
            k: args[0].to_number(),
        })])
    }))
}

/// `force/gravity`: uniform acceleration along `direction` (× mass).
fn gravity(init: &NodeInit) -> Box<dyn NodeImpl> {
    memo(scalar(init, |args| {
        let d = vec2_or(&args[0], Vec2 { x: 0.0, y: 1.0 });
        let s = args[1].to_number();
        let len = js_hypot(d.x, d.y);
        let len = if len == 0.0 || len.is_nan() { 1.0 } else { len };
        Ok(vec![force(ForceKind::Gravity {
            ax: (d.x / len) * s,
            ay: (d.y / len) * s,
        })])
    }))
}

/// `force/attractor`: radial pull toward `center` within `radius`.
fn attractor(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo(raw(|inputs, _cx| {
        let a = radial(inputs);
        let (cx, cy) = (a.center.x, a.center.y);
        let color = if a.strength >= 0.0 { "#7fdfff" } else { "#ff7f7f" };
        Ok(vec![
            force_spread(ForceKind::Attractor {
                cx,
                cy,
                strength: a.strength,
                r: a.r,
                inv_r: 1.0 / a.r,
                curve: a.curve,
            }),
            viz(a.show_viz, cx, cy, a.r, color),
        ])
    }))
}

/// `force/repulsor`: radial push away from `center` within `radius`.
fn repulsor(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo(raw(|inputs, _cx| {
        let a = radial(inputs);
        let (cx, cy) = (a.center.x, a.center.y);
        Ok(vec![
            force_spread(ForceKind::Repulsor {
                cx,
                cy,
                strength: a.strength,
                r: a.r,
                inv_r: 1.0 / a.r,
                curve: a.curve,
            }),
            viz(a.show_viz, cx, cy, a.r, "#ff7f7f"),
        ])
    }))
}

/// `force/vortex`: tangential push plus radial pull around `center`.
fn vortex(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo(raw(|inputs, _cx| {
        let center = vec2_or(&first(inputs, 0), Vec2 { x: 0.0, y: 0.0 });
        let axis = num_or(&first(inputs, 1), 200.0);
        let pull = num_or(&first(inputs, 2), 50.0);
        let r = js::max(0.01, num_or(&first(inputs, 3), 200.0));
        let curve = FalloffCurve::coerce(&first(inputs, 4));
        let show_viz = first(inputs, 5).truthy();
        Ok(vec![
            force_spread(ForceKind::Vortex {
                cx: center.x,
                cy: center.y,
                axis,
                pull,
                r,
                inv_r: 1.0 / r,
                curve,
            }),
            viz(show_viz, center.x, center.y, r, "#bf7fff"),
        ])
    }))
}

/// `force/turbulence`: Perlin vector field drifting with the force clock.
fn turbulence(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo(raw(|inputs, _cx| {
        Ok(vec![force_spread(ForceKind::Turbulence {
            scale: num_or(&first(inputs, 0), 0.01),
            speed: num_or(&first(inputs, 1), 0.5),
            strength: num_or(&first(inputs, 2), 300.0),
            seed: num_or(&first(inputs, 3), 0.0).floor(),
        })])
    }))
}

/// `force/wind`: directional force with Perlin magnitude noise.
fn wind(_: &NodeInit) -> Box<dyn NodeImpl> {
    memo(raw(|inputs, _cx| {
        let dir = vec2_or(&first(inputs, 0), Vec2 { x: 1.0, y: 0.0 });
        let len = js_hypot(dir.x, dir.y);
        let len = if len == 0.0 || len.is_nan() { 1.0 } else { len };
        Ok(vec![force_spread(ForceKind::Wind {
            dx: dir.x / len,
            dy: dir.y / len,
            strength: num_or(&first(inputs, 1), 150.0),
            noise_amp: num_or(&first(inputs, 2), 0.2),
            seed: num_or(&first(inputs, 3), 0.0).floor(),
        })])
    }))
}
