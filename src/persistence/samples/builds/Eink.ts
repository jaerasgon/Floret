import { node, edge, note } from '../helpers';

// ─── E-ink Clock ────────────────────────────────────────────────────────
// A bold sweeping hand inside a ring — high-contrast black-on-white that
// reads cleanly on a 1-bit e-paper panel. The drawing fans into BOTH the
// Renderer (on-screen preview) and a generic `io/send` node on channel
// "eink", which rasterises the layers to a PNG and POSTs them to the in-app
// bus. A Raspberry Pi (or another Floret tab via io/receive) pulls that
// channel — nothing here is e-ink-specific.
export function buildEinkClock() {
  const c = note(
    'ek_c',
    { x: -360, y: -40 },
    'Send / receive\n\nThe drawing fans into the Renderer (preview) AND a generic Send node on channel "eink" — it rasterises the layers to a PNG onto the in-app bus. Anything that pulls that channel paints it: a Raspberry Pi, or another Floret tab with a Receive node.\n\nsend toggle = push now · interval = auto-push (seconds). width/height set the sent resolution.',
    { w: 520, h: 220 },
  );
  const tm = node('ek_tm', 'time/seconds', { x: -360, y: 80 });
  const mul = node('ek_mul', 'math/mul', { x: -200, y: 80 }, { b: 0.5 });          // angle = t * 0.5
  const hand = node('ek_hand', 'vec2/from-angle', { x: -40, y: 80 }, { mag: 46 }); // hand tip vector
  const blk = node('ek_blk', 'color/color', { x: -360, y: 200 }, { mode: 'rgb', r: 0, g: 0, b: 0, a: 255 });
  const stk = node('ek_stk', 'style/stroke', { x: -200, y: 200 }, { width: 6 });
  const ring = node('ek_ring', 'draw/ellipse', { x: 140, y: -40 }, { w: 100, h: 100 });
  const ln = node('ek_ln', 'draw/line', { x: 140, y: 120 });
  const wht = node('ek_wht', 'color/color', { x: 140, y: 260 }, { mode: 'rgb', r: 255, g: 255, b: 255, a: 255 });
  // Render at the panel's resolution (250x122 landscape; the Pi rotates it
  // to the native 122x250) so the editor preview is exactly what's sent.
  const rnd = node('ek_rnd', 'render/renderer', { x: 400, y: -40 }, { width: 250, height: 122 });
  const tog = node('ek_tog', 'io/toggle', { x: 400, y: 250 }, { value: true });
  const send = node('ek_send', 'io/send', { x: 400, y: 120 },
    { channel: 'eink', interval: 2, width: 250, height: 122 });
  return {
    nodes: [c, tm, mul, hand, blk, stk, ring, ln, wht, rnd, tog, send],
    edges: [
      edge('ek_tm', 't', 'ek_mul', 'a'),
      edge('ek_mul', 'result', 'ek_hand', 'angle'),
      edge('ek_hand', 'vector', 'ek_ln', 'b'),
      edge('ek_blk', 'color', 'ek_stk', 'color'),
      edge('ek_stk', 'style', 'ek_ring', 'style'),
      edge('ek_stk', 'style', 'ek_ln', 'style'),
      // fan the two shapes into the Renderer (preview) and the Send node
      edge('ek_ring', 'shape', 'ek_rnd', 'layers'),
      edge('ek_ln', 'shape', 'ek_rnd', 'layers'),
      edge('ek_ring', 'shape', 'ek_send', 'data'),
      edge('ek_ln', 'shape', 'ek_send', 'data'),
      edge('ek_wht', 'color', 'ek_rnd', 'background'),
      edge('ek_wht', 'color', 'ek_send', 'background'),
      edge('ek_tog', 'bool', 'ek_send', 'send'),
    ],
  };
}
