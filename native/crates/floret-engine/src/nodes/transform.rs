//! `transform/*` nodes. Ported from `src/nodes/transform/` (`translate.ts`,
//! `affine.ts`, `blend.ts`, `group.ts`).
//!
//! Each wraps its inner layer in a `group` scene node carrying an affine
//! transform or a blend mode; `transform/group` concatenates N layer slots.
//!
//! Not ported yet: `transform/cache` (it rasterises into an image).
//!
//! Native-only: `transform/mask`.

use super::shared::to_vec2;
use super::Registrar;
use crate::node::{raw, scalar, NodeImpl, NodeInit};
use crate::scene::{rotate_mat, scale_mat, translate_mat, Affine, SceneNode};
use crate::spec::{Params, PortKind, PortSpec, PortType};
use crate::value::{one, Value};
use std::sync::Arc;

pub fn register(r: &mut Registrar) {
    r.node("transform/translate", translate);
    r.node("transform/rotate", rotate);
    r.node("transform/scale", scale);
    r.node("transform/shear-x", shear_x);
    r.node("transform/shear-y", shear_y);
    r.node("transform/apply-matrix", apply_matrix);
    r.node("transform/blend", blend);
    r.node("transform/group", group);
    r.dynamic("transform/group", build_layer_slots);
    r.node("transform/mask", mask);
}

/// The scene nodes among a wire's values. TS puts whatever arrived into
/// `children` (an unwired layer is `null`); the renderer skips non-shapes,
/// and a `SceneNode` child can only be a shape, so others are dropped.
fn scenes<'a>(values: impl IntoIterator<Item = &'a Value>) -> Vec<Arc<SceneNode>> {
    values
        .into_iter()
        .filter_map(|v| match v {
            Value::Scene(n) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

fn wrap(layer: &Value, transform: Option<Affine>, blend_mode: Option<&str>) -> Value {
    Value::Scene(Arc::new(SceneNode::Group {
        children: Arc::from(scenes([layer])),
        transform,
        blend_mode: blend_mode.map(Arc::from),
        clip: None,
    }))
}

// ─── translate.ts ────────────────────────────────────────────────────────────

fn translate(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let v = to_vec2(&args[0]);
        Ok(vec![wrap(&args[1], Some(translate_mat(v.x, v.y)), None)])
    })
}

fn rotate(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let m = rotate_mat(args[0].to_number());
        Ok(vec![wrap(&args[1], Some(m), None)])
    })
}

fn scale(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let m = scale_mat(args[0].to_number(), args[1].to_number());
        Ok(vec![wrap(&args[2], Some(m), None)])
    })
}

// ─── affine.ts ───────────────────────────────────────────────────────────────

fn shear_x(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let m = [1.0, 0.0, args[0].to_number().tan(), 1.0, 0.0, 0.0];
        Ok(vec![wrap(&args[1], Some(m), None)])
    })
}

fn shear_y(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let m = [1.0, args[0].to_number().tan(), 0.0, 1.0, 0.0, 0.0];
        Ok(vec![wrap(&args[1], Some(m), None)])
    })
}

fn apply_matrix(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let m: Affine = [0, 1, 2, 3, 4, 5].map(|i| args[i].to_number());
        Ok(vec![wrap(&args[6], Some(m), None)])
    })
}

// ─── blend.ts ────────────────────────────────────────────────────────────────

/// `toCanvasBlend`: patch-facing blend names → canvas composite operations.
fn to_canvas_blend(mode: &str) -> &'static str {
    match mode {
        "add" => "lighter",
        "darkest" => "darken",
        "lightest" => "lighten",
        "difference" => "difference",
        "exclusion" => "exclusion",
        "multiply" => "multiply",
        "screen" => "screen",
        "replace" => "copy",
        "overlay" => "overlay",
        "hard-light" => "hard-light",
        "soft-light" => "soft-light",
        "dodge" => "color-dodge",
        "burn" => "color-burn",
        // Closest blend-mode-set approximation.
        "subtract" => "difference",
        _ => "source-over",
    }
}

fn blend(init: &NodeInit) -> Box<dyn NodeImpl> {
    scalar(init, |args| {
        let mode = match &args[0] {
            Value::Str(s) => to_canvas_blend(s),
            _ => "source-over",
        };
        Ok(vec![wrap(&args[1], None, Some(mode))])
    })
}

// ─── group.ts ────────────────────────────────────────────────────────────────

/// Upper bound on slots. TS has none (an infinite `slotCount` hangs it).
const MAX_SLOTS: f64 = 4096.0;

/// `buildLayerSlots`: `slotCount` layer-spread pins `s1`, `s2`, ….
fn build_layer_slots(params: &Params) -> Vec<PortSpec> {
    let raw = params
        .get("slotCount")
        .map_or(Value::Undefined, Value::from_json)
        .to_number();
    // `Number(x) || 2`, floored, at least 1.
    let count = if raw.is_nan() || raw == 0.0 { 2.0 } else { raw };
    let n = count.floor().clamp(1.0, MAX_SLOTS) as usize;
    (1..=n)
        .map(|i| {
            PortSpec::new(format!("s{i}"), PortType::Layer)
                .label(i.to_string())
                .kind(PortKind::Spread)
        })
        .collect()
}

fn group(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, _| {
        let children = scenes(inputs.iter().flat_map(|s| s.iter()));
        Ok(vec![one(Value::Scene(Arc::new(SceneNode::group(
            children,
        ))))])
    })
}

// ─── transform/mask (native) ─────────────────────────────────────────────────

/// `layer` shown through `mask` (both spreads of layers, each drawn as one
/// group). The `mode` param picks alpha or luminance coverage; `invert`
/// shows the layer where the mask is empty. An empty mask hides the whole
/// layer (inverted: shows all of it).
fn mask(_: &NodeInit) -> Box<dyn NodeImpl> {
    raw(|inputs, cx| {
        let layer = |i: usize| {
            let nodes = scenes(inputs.get(i).map_or(&[][..], |s| &s[..]));
            Arc::new(SceneNode::group(nodes))
        };
        let luminance = matches!(
            cx.param("mode"),
            Some(serde_json::Value::String(s)) if s == "luminance"
        );
        let invert = inputs
            .get(2)
            .and_then(|s| s.first())
            .is_some_and(Value::truthy);
        Ok(vec![one(Value::Scene(Arc::new(SceneNode::Mask {
            content: layer(0),
            mask: layer(1),
            luminance,
            invert,
        })))])
    })
}

#[cfg(test)]
mod tests {
    use super::super::value::harness::node;
    use super::*;
    use crate::scene::default_style;

    #[test]
    fn mask_wraps_layer_and_mask() {
        let dot = Value::Scene(Arc::new(SceneNode::Point {
            x: 1.0,
            y: 2.0,
            style: default_style().clone(),
        }));
        let mut h = node("transform/mask", serde_json::json!({ "mode": "luminance" }));
        let out = h
            .eval(vec![
                vec![dot.clone(), dot.clone()],
                vec![dot],
                vec![Value::Bool(true)],
            ])
            .unwrap();
        let Value::Scene(n) = &out[0][0] else {
            panic!("not a layer")
        };
        let SceneNode::Mask {
            content,
            mask,
            luminance,
            invert,
        } = &**n
        else {
            panic!("not a mask: {n:?}")
        };
        assert!(*luminance && *invert);
        assert!(matches!(&**content, SceneNode::Group { children, .. } if children.len() == 2));
        assert!(matches!(&**mask, SceneNode::Group { children, .. } if children.len() == 1));
        // Defaults: alpha, not inverted.
        let mut h = node("transform/mask", serde_json::json!({}));
        let out = h.eval(vec![vec![], vec![], vec![]]).unwrap();
        let Value::Scene(n) = &out[0][0] else {
            panic!("not a layer")
        };
        assert!(matches!(
            &**n,
            SceneNode::Mask {
                luminance: false,
                invert: false,
                ..
            }
        ));
    }

    fn slots(j: serde_json::Value) -> Vec<String> {
        let params: Params = serde_json::from_value(j).unwrap();
        build_layer_slots(&params)
            .into_iter()
            .map(|p| format!("{}:{}", p.name, p.label.unwrap_or_default()))
            .collect()
    }

    #[test]
    fn layer_slots_follow_slot_count() {
        assert_eq!(slots(serde_json::json!({})), ["s1:1", "s2:2"]);
        assert_eq!(slots(serde_json::json!({"slotCount": 0})), ["s1:1", "s2:2"]);
        assert_eq!(slots(serde_json::json!({"slotCount": -3})), ["s1:1"]);
        assert_eq!(slots(serde_json::json!({"slotCount": "3.7"})).len(), 3);
        let p: Params = serde_json::from_value(serde_json::json!({"slotCount": 4})).unwrap();
        let ports = build_layer_slots(&p);
        assert_eq!(ports.len(), 4);
        assert!(ports
            .iter()
            .all(|p| p.ty == PortType::Layer && p.kind == PortKind::Spread && p.default.is_none()));
    }

    #[test]
    fn blend_names() {
        assert_eq!(to_canvas_blend("add"), "lighter");
        assert_eq!(to_canvas_blend("dodge"), "color-dodge");
        assert_eq!(to_canvas_blend("blend"), "source-over");
        assert_eq!(to_canvas_blend("nope"), "source-over");
    }
}
