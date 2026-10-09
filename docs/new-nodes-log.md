# New nodes log

Capabilities the Library samples needed that Floret lacked, why, and what
was built. One section per piece of work.

## Interactive samples (agent)

Fifteen interaction-first samples (Library categories **Interactive** and
**Sound**: `play-*`, `sound-*`). Most of what they do is built from existing
nodes — Expression over spreads with State / State* for game logic,
nearest-point picking (Expression `distance` → Statistics `min` → Lookup
`index-of`), click edges (`pressed && !was` with a State), list editing with
Combine / Sublist. Three things could not be built from nodes at all:

### Keyboard: held keys and typed text (`input/keyboard` modes)

- **Missing:** the Keyboard node only knew the *last* key pressed and whether
  *any* key was down. A piano needs to know which keys are held (chords, the
  latest press for a mono synth, a re-trigger on each new press); a typing
  toy needs the characters typed — case, punctuation, Backspace — which the
  last key can't rebuild (pressing the same key twice, keys overlapping).
- **Design:** no new node — a `mode` dropdown on Keyboard (pins via
  `DynamicInputsFn`): `last` (unchanged, no pins), `keys` (a `keys` string:
  one character per key, or names split by commas; outputs `down*` per key
  and `index`, the latest press still held) and `text` (`clear`, `max`;
  output `text`, kept across patch edits). A `hit` output fires on the frame
  a key goes down in every mode. The frame carries two new fields,
  `FrameCtx::keys_down` and `FrameCtx::typed` (egui `Text` events, `\n` for
  Enter, `\u{8}` for Backspace), filled by the preview. Keyboard turned from a
  context node into an effectful raw node (it keeps state); `last` mode
  outputs exactly what it did (parity compares its first two outputs).
- **Used by:** sound-keyboard-synth, play-typing-toy, and the key controls of
  nearly every sample (brush, sequencer, sandbox, voronoi, flock,
  kaleidoscope, reaction, MIDI garden fallback, music box).

### Scale (`sound/scale`)

- **Missing:** turning a mouse position, a noise value, a grid row or a hand
  into notes *in key* took a Lookup per scale and octave arithmetic in every
  patch, with no way to switch scales.
- **Design:** one node, `mode` degree / quantize / range, `scale` a dropdown
  pin that can be wired (an `io/select` on the canvas switches it live) —
  12 scales plus `custom` (`steps`). Per element over spreads; outputs MIDI
  `note`, `freq` and the `index` in the scale (for colours). Pure, memoised.
- **Used by:** sound-step-sequencer (rows → degrees), sound-hand-theremin
  (x → range, snapped or gliding), sound-music-box (rings → degrees),
  sound-mic-bloom (its demo loop), and the theremin's lane display.

### Mix (`sound/mix`)

- **Missing:** every audio processor and the Output take one input, so
  several voices could only reach the speakers through separate Outputs —
  they couldn't share a filter, delay, reverb or an FFT.
- **Design:** `sound/mix` with `slotCount` audio inputs (`− n +` in the
  header) and `level`. The engine declares `AudioNode::Mix { inputs, level }`;
  floret-media orders it after all its inputs and sums them into a scratch
  bus on the audio thread (no allocation), then applies the level ramp like
  Gain.
- **Used by:** sound-step-sequencer (4 voices), sound-keyboard-synth (lead +
  sub), sound-hand-theremin, sound-mic-bloom (mic + demo → FFT), sound-music-box
  (3 bells, each two partials).

### Suggestion (not done: UI behaviour)

While the pointer is over a preview, the node canvas still acts on its
single-key shortcuts — F fits, Tab opens search, Q the favourites menu,
Backspace / Delete delete the selection — and the preview takes Space for
pause. Keyboard-played samples work (the patch reads keys before the
canvas does) but the editor reacts too; their comments suggest Focus mode.
Letting the preview own plain keys while it is hovered would fix it, but
changes existing UI behaviour, so it is left for the owner to decide.
