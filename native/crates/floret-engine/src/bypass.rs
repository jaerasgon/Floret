//! Node bypass: a node switched off in the editor (`bypassed: true` on
//! the node in the `.flora` file) keeps its pins but stops computing.
//!
//! Each output passes through the first input of the same type, as it
//! arrives (wired value, or the input's own default when unwired), so an
//! Effect hands its input layer on untouched, a Math node set to
//! multiply passes `a` through, an Image filter its image. An output with
//! no input of its type gives the empty / neutral value: nothing for
//! layers, images, audio, styles and the particle types; the type's
//! default (0, false, "", white, the origin) for plain values, so
//! downstream maths keeps running.
//!
//! Only regular compute nodes can be bypassed (see [`can_bypass`]):
//! regions and their plumbing (splicers, accumulators, loop variables),
//! State nodes (their feedback loop would become a cycle), comments,
//! probes (already pass-throughs) and the Renderer (the patch's output)
//! can't.

use crate::compile::default_for_port;
use crate::node::{EvalCtx, NodeImpl, NodeResult};
use crate::spec::{EvalKind, NodeSpec, PortSpec, PortType};
use crate::value::{empty, Spread};

/// Whether nodes of this type can be bypassed.
pub fn can_bypass(spec: &NodeSpec) -> bool {
    spec.region_kind.is_none()
        && spec.region_role.is_none()
        && !spec.stateful
        && spec.eval != EvalKind::None
        && !spec.ty.starts_with("state/")
        && !matches!(spec.ty.as_str(), "meta/probe" | "render/renderer")
}

/// Where each output takes its value from while bypassed.
#[derive(Clone, Debug)]
pub enum Route {
    /// The input at this index, passed straight through.
    Input(usize),
    /// A fixed value (empty or the type's default).
    Value(Spread),
}

/// The routes for a node with these (resolved) pins: per output, the
/// first input of the same type, else [`neutral`].
pub fn routes(inputs: &[PortSpec], outputs: &[PortSpec]) -> Vec<Route> {
    outputs
        .iter()
        .map(|out| match inputs.iter().position(|i| i.ty == out.ty) {
            Some(i) => Route::Input(i),
            None => Route::Value(neutral(out)),
        })
        .collect()
}

/// What an output with nothing to pass through gives.
pub fn neutral(out: &PortSpec) -> Spread {
    match out.ty {
        PortType::Layer
        | PortType::Image
        | PortType::Audio
        | PortType::Style
        | PortType::Any
        | PortType::World
        | PortType::Force
        | PortType::Collider
        | PortType::Particle => empty(),
        _ => default_for_port(out),
    }
}

/// The compute a bypassed node runs instead of its own.
pub struct Bypass {
    routes: Vec<Route>,
}

impl Bypass {
    pub fn new(inputs: &[PortSpec], outputs: &[PortSpec]) -> Bypass {
        Bypass {
            routes: routes(inputs, outputs),
        }
    }
}

impl NodeImpl for Bypass {
    fn eval(&mut self, inputs: &[Spread], _cx: &mut EvalCtx) -> NodeResult {
        Ok(self
            .routes
            .iter()
            .map(|r| match r {
                Route::Input(i) => inputs.get(*i).cloned().unwrap_or_else(empty),
                Route::Value(v) => v.clone(),
            })
            .collect())
    }

    /// A pass-through is pure, but it's also nearly free; running it
    /// every tick keeps image / audio inputs (whose contents change under
    /// the same handle) flowing through.
    fn memoise(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::PatchFile;
    use crate::value::Value;
    use crate::{Catalogue, Engine, FrameCtx};
    use serde_json::json;

    fn spec(ty: &str) -> &'static NodeSpec {
        Catalogue::builtin().get(ty).expect(ty)
    }

    #[test]
    fn only_regular_compute_nodes_bypass() {
        for ty in ["gpu/effect", "math/math", "draw/ellipse", "time/seconds"] {
            assert!(can_bypass(spec(ty)), "{ty}");
        }
        for ty in [
            "region/foreach",
            "region/if",
            "region/cache",
            "meta/subpatch",
            "meta/comment",
            "meta/probe",
            "render/renderer",
            "state/value",
            "state/read",
            "state/write",
        ] {
            assert!(!can_bypass(spec(ty)), "{ty}");
        }
    }

    #[test]
    fn outputs_take_the_first_input_of_their_type() {
        let cat = Catalogue::builtin();
        let effect = spec("gpu/effect");
        let params = effect.default_params.clone();
        let r = routes(
            &cat.resolved_inputs(effect, &params),
            &cat.resolved_outputs(effect, &params),
        );
        assert!(matches!(r.as_slice(), [Route::Input(0)]));
        // An ellipse has no layer input: its layer output goes empty.
        let ellipse = spec("draw/ellipse");
        let r = routes(&ellipse.inputs, &ellipse.outputs);
        assert!(matches!(&r[0], Route::Value(v) if v.is_empty()));
        // A number output with nothing to pass gives the type's default.
        let seconds = spec("time/seconds");
        let r = routes(&seconds.inputs, &seconds.outputs);
        assert!(
            matches!(&r[0], Route::Value(v) if matches!(v.first(), Some(Value::Num(n)) if *n == 0.0))
        );
    }

    fn patch(bypassed: bool) -> PatchFile {
        let text = json!({
            "kind": "floret.patch",
            "version": 3,
            "nodes": [
                { "id": "a", "defType": "io/slider", "position": { "x": 0, "y": 0 },
                  "params": { "value": 3, "min": 0, "max": 10 } },
                { "id": "m", "defType": "math/math", "position": { "x": 200, "y": 0 },
                  "params": { "op": "multiply" }, "portValues": { "b": 4 },
                  "bypassed": bypassed }
            ],
            "edges": [
                { "id": "e1", "source": "a", "target": "m",
                  "sourceHandle": "out-number", "targetHandle": "in-a" }
            ]
        });
        crate::load_patch(&text.to_string()).expect("loads")
    }

    fn result(engine: &mut Engine) -> f64 {
        let out = engine.tick(&FrameCtx::default()).outputs;
        let i = engine.graph.idx("m").unwrap();
        match out[i].as_ref().unwrap()[0][0] {
            Value::Num(n) => n,
            ref v => panic!("{v:?}"),
        }
    }

    #[test]
    fn a_bypassed_node_passes_its_input_through() {
        let (mut on, _) = Engine::from_patch(&patch(false)).unwrap();
        assert_eq!(result(&mut on), 12.0);
        let (mut off, _) = Engine::from_patch(&patch(true)).unwrap();
        assert_eq!(result(&mut off), 3.0);
        // A live edit upstream still flows through every tick (nothing
        // memoised holds the old value).
        off.params.set("a", "value", json!(7));
        assert_eq!(result(&mut off), 7.0);
        assert_eq!(result(&mut off), 7.0);
    }

    #[test]
    fn bypassed_round_trips_and_defaults_to_off() {
        let p = patch(true);
        assert!(p.nodes[1].bypassed && !p.nodes[0].bypassed);
        let text = p.to_json();
        assert!(text.contains("\"bypassed\": true") || text.contains("\"bypassed\":true"));
        // Absent when off, so files from builds without bypass match.
        assert_eq!(text.matches("bypassed").count(), 1);
        let back = crate::load_patch(&text).unwrap();
        assert!(back.nodes[1].bypassed);
    }

    #[test]
    fn regions_ignore_the_flag() {
        let text = json!({
            "kind": "floret.patch",
            "version": 3,
            "nodes": [
                { "id": "r", "defType": "region/foreach", "position": { "x": 0, "y": 0 },
                  "params": {}, "bypassed": true, "width": 300, "height": 200 }
            ],
            "edges": []
        });
        let p = crate::load_patch(&text.to_string()).unwrap();
        let (mut e, _) = Engine::from_patch(&p).unwrap();
        e.tick(&FrameCtx::default());
        let i = e.graph.idx("r").unwrap();
        assert!(e.graph.nodes[i].region_kind.is_some());
    }
}
