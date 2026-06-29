# GPU Shader Render Node (GLSL + feedback) — Design

Issue: [#22](https://github.com/Jaerasgon/Floret/issues/22). Status: design approved (user delegated validation + execution). Date: 2026-06-29.

## Goal

A fragment-shader node that renders pixels into a Floret patch — ShaderToy-style
authoring, optional scene input, graph-wired uniforms, and multi-pass feedback
(reaction-diffusion, trails, post-FX, raymarching). Builds on the WebGL2 renderer
delivered this cycle (the issue's "no WebGL path" premise is already solved).

## Decisions (from brainstorming)

| Question | Decision |
|---|---|
| Purpose | Both: input optional (post-FX when wired, generative when not). |
| Dialect | ShaderToy entry (`mainImage`, `iTime`/`iResolution`/`iMouse`/`iFrame`/`iChannelN`) **plus** graph-wired uniforms. |
| Feedback | Multi-buffer (Buffer A–D), via a **scalable** node (1 pass default → up to 4 buffers). |
| Buffer model | **Self-contained node**: passes + feedback live inside one node; no graph back-edges. |
| Editing | **Pinned side-panel** editor (highlight + line-mapped errors + uniform list). |
| Uniforms | **Auto-detected** from `uniform` declarations → typed input ports. Built-ins provided automatically. |
| Run location | **Render-integrated** (approach A): executed by the GL renderer in the per-preview worker, where time + frames already live. |

## Architecture

A shader node renders pixels every frame with feedback, so it lives in the
**render loop**, not the tick. The graph node emits a `{ kind:'shader' }`
SceneNode; the **GL renderer** executes it inside the same WebGL2 context that
already runs in the per-preview worker and the shared main-thread context.

### Data flow (per frame, in the renderer)

```
shader SceneNode → for each pass [Buffer A..D (if any), then Image]:
   bind pass FBO → run pass shader, sampling its channels
     (wired input layer · sibling buffers · own previous-frame buffer)
   ping-pong that buffer's two FBOs
→ Image-pass output texture → composited into the scene as a textured quad
```

Single-pass shaders skip straight to the Image pass. A previous-frame buffer
gives self-feedback with no graph back-edge.

### Renderer reuse vs. additions

- **Reuse:** `acquireLayer()` (full-viewport FBO pool) for the input-layer render
  and transient pass targets; `composite(tex, blend)` to blit the result; the
  `pushLayer → render children → use as texture` path to rasterize the wired
  input layer into iChannel0.
- **Add:** a per-node **persistent** FBO map (`Map<nodeId, PingPong>`) for
  feedback buffers (the pool is transient/LIFO; feedback must survive frames).
  Keyed by node id + buffer index; reallocated on resolution change. Disposed
  when the node leaves the graph (retain/dispose like the image registries).
- **Add:** a shader-program cache keyed by source hash (compile once, reuse).

### ShaderToy → WebGL2 wrapper

User writes `void mainImage(out vec4 fragColor, in vec2 fragCoord)`. We prepend a
preamble and append a `main()`:

```glsl
#version 300 es
precision highp float;
uniform float iTime; uniform float iTimeDelta; uniform int iFrame;
uniform vec3 iResolution; uniform vec4 iMouse;
uniform sampler2D iChannel0, iChannel1, iChannel2, iChannel3;
// + auto-detected user uniforms injected here
out vec4 _fragColor;
// <user source>
void main(){ mainImage(_fragColor, gl_FragCoord.xy); }
```

`texture()`, `texelFetch`, `textureLod` are GLSL ES 3.00-native, so most
ShaderToy Image/Buffer shaders port unchanged. Compile failures are caught and
surfaced (below), never thrown into the frame.

## Components

### 1. Node `gpu/shader`
- **Param `source`** (GLSL string) — the Image pass. Buffer-pass sources
  (`bufferA`..`bufferD`) added as the node scales (Phase 2).
- **`dynamicInputs(params)`** — lightweight parse of `uniform <type> <name>;`
  declarations → typed ports: `float`→number, `vec2`→vec2, `vec3`/`vec4`→color,
  `sampler2D`→layer (an extra iChannel). ShaderToy built-ins are recognised by
  name and **never** become ports. Always-present port: `input` (the primary
  layer → iChannel0).
- **Output** `out` (`image`/`layer`).
- **`compute()`** resolves wired uniform values + the input-layer subtree, emits
  `{ kind:'shader', id, source, passes, channels, uniforms, resolution }`.
- Carries `stateful`-style identity so the renderer can key feedback FBOs by node id.

### 2. SceneNode `ShaderShape` (scene.ts)
```ts
interface ShaderShape {
  kind: 'shader';
  nodeId: string;                 // feedback-FBO key
  passes: ShaderPass[];           // [Image] in P1; [A..D, Image] in P2
  input: SceneNode | null;        // iChannel0 source subtree (or null = generative)
  uniforms: Record<string, number | [number,number] | [number,number,number,number]>;
}
interface ShaderPass { source: string; channels: ChannelBinding[]; feedback: boolean; }
```

### 3. Renderer pipeline (gl-vector-renderer)
- `FrameBuilder.pushShader(shape)` → a `shader` command.
- Compiles/caches the wrapped program; manages the node's persistent ping-pong
  FBO(s); binds channels (input texture, sibling buffers, previous-frame);
  sets built-in + user uniforms; draws a fullscreen triangle per pass; composites
  the Image output.
- Built-ins fed from the render frame: `iTime`, `iTimeDelta`, `iFrame`,
  `iResolution` (viewport), `iMouse` (from the existing mouse plumbing).

### 4. Editor side-panel `ShaderPanel` (React)
- Modeled on `HelpPanel`. Opens when a `gpu/shader` node is selected.
- **Phase 1:** monospace `<textarea>`, **line-mapped compile errors** (parsed
  from the GL `getShaderInfoLog`, offset-adjusted for the preamble line count),
  detected-uniform list. Edits write the `source` param (debounced) → re-parse
  ports + recompile.
- **Phase 2:** buffer tabs (Image + A–D), per-channel routing dropdowns.
- **Fast-follow:** CodeMirror 6 syntax highlighting (decision deferred to keep
  the dep out of the critical path).

### Error handling
- Compile error → errors shown in panel; the node renders a **safe fallback**
  (passthrough of the input layer if present, else a flat error color). The
  patch never hard-breaks.
- No input wired → generative shaders render fine; iChannel0 binds a 1×1
  transparent texture.

## Scope / phasing

**Phase 1 (MVP, this plan):** single pass + **self-feedback** (a previous-frame
buffer exposed as iChannel1, opt-in). ShaderToy Image entry, built-ins, one wired
input, auto-uniform ports, side-panel editor (functional, no highlight yet),
output layer, `kind:'shader'` + renderer pipeline + persistent feedback FBO. A
showcase sample. Covers post-FX, generative, **and** reaction-diffusion/trails —
independently shippable.

**Phase 2 (next plan):** the rest of the multi-buffer model — Buffer A–D passes
(N passes on the same machinery), channel-routing UI, per-buffer ping-pong;
optional CodeMirror highlighting.

## Testing
- **Headless (vitest):** uniform-parse (declarations → port specs, built-ins
  excluded); wrapper assembly; compile-error line-mapping; SceneNode emission
  shape. No real GL in headless — degrade gracefully (existing pattern).
- **Preview GL harness (real WebGL2):** a known shader → known output pixels; a
  feedback shader accumulates across frames (alpha/area grows); compile error →
  fallback renders. Mirrors how the renderer fixes were verified this cycle.

## Out of scope (v1)
Compute shaders; vertex shaders; #include / multi-file; geometry from shaders;
audio-texture iChannels (FFT-as-texture) — revisit after Phase 2.
