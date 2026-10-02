# Golden fixtures

Recorded from the TypeScript app (the reference implementation) by
`npm run fixtures:export` (`tools/fixtures/export.fixture.ts`). The Rust
engine must reproduce them. Don't edit by hand; re-export instead.

| File | Contents |
|---|---|
| `registry.json` | Every node type: ports (name, type, kind, default, `param`, `constant`), eval kind, flags |
| `samples/<sample>.flora` | Each traced sample's patch exactly as the trace recorded it |
| `traces/<sample>.json.gz` | For each sample in `samples/`: compile result (warnings, scope order, runtime nodes, edges), the frame input for every tick (`inputs`), and every node's outputs at frames 0, 1, 2, 10, 30, 60 |

## Frozen samples

The parity test (`crates/floret-engine/tests/parity.rs`) compiles
`samples/<id>.flora` from this folder, not the app's library in
`../samples/`. A trace names every node of the graph it was recorded from,
so parity needs that graph. The library samples have since been reworked
(fewer nodes, the same pictures) and no longer match the traces node for
node. They're checked by `tests/all_samples.rs` instead: each one compiles
without warnings, runs and draws.

## Run conditions

- 61 ticks per sample, `dt = 1/60`, `t = frame × dt`.
- Scripted input: mouse circles the centre (centre-origin coords), pressed on
  frames 20–39 of every 40; key `a` from frame 10, held on frames 10–19.
- Canvas size comes from the first renderer's `width` / `height`, clamped to
  32–4096 (same as the app's driver).
- `Math.random` is seeded (mulberry32, seed `0x5eed`) and clocks are mocked,
  so the run is deterministic.

## Value encoding

Plain JSON, plus tags for things JSON can't express:

| Tag | Meaning |
|---|---|
| `{"$u":1}` | `undefined` (distinct from `null`) |
| `{"$num":"NaN"}` | NaN / `Infinity` / `-Infinity` |
| `{"$style":{…}}` | a Style closure, shown as the result of applying it to `NEUTRAL_STYLE` |
| `{"$fn":"name"}` | any other function |
| `{"$ta":"Float32Array","len":n,"fnv":"…","head":[…]}` | typed array: length, FNV-1a of the bytes, first 8 values |
| `{"$map":[[k,v],…]}`, `{"$set":[…]}` | Map / Set |
| `{"$opaque":"Name"}` | browser resource (canvas, image, audio node) |
| `{"$class":"Name",…}` | non-plain object instance |
| `{"$id":n,…}` / `{"$id":n,"$arr":[…]}` then `{"$ref":n}` | an object / array reached more than once in one frame: written once, referenced after |
| `{"$cycle":1}` | reference back to an object still being written |
| `{"$summary":{objects,numbers,sum,abs,strings,kinds}}` | an output with more than 200k objects, summarised over the value as a tree |

## Known browser-only gaps

Running outside a browser, these samples can't produce their media:

- `kaleidoscope`, `datamosh`, `photo-mosaic`, `pixel-sort`: an image node throws
  `Image is not defined` on frame 0, so the trace stops there (`tickError`).
- Sound samples (`tut-28`, `tut-29`, `tut-46`–`tut-49`, `tone-surface`,
  `fft-bloom`) and camera samples (`webcam-mosaic`, `body-tracking`): media
  nodes output nothing; listed in the trace's `errors`.

Their non-media nodes are still recorded and still compared.
