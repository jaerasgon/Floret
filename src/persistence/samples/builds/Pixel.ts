import { node, edge } from '../helpers';



// ─── Sample: Pixel Sort ───────────────────────────────────────────────────
// Classic glitch art: for each row of a photo, identify runs of pixels whose
// brightness falls in the (lo, hi) range and sort them. Pixels outside the
// mask stay put — that's what gives pixel-sort its streaked, "leaking" look
// rather than a smooth gradient. Sliders drive the mask bounds live.
export function buildPixelSort() {
  const img      = node('ps_img',    'io/image-url', { x: -400, y:  20 },
                        { url: 'https://picsum.photos/seed/pixel-sort/500/500' });
  const time     = node('ps_time',   'time/seconds', { x: -400, y: 100 });
  const lo       = node('ps_lo',     'io/slider',    { x: -400, y: 180 }, { value: 0.0,  min: 0, max: 1 });
  const hi       = node('ps_hi',     'io/slider',    { x: -400, y: 240 }, { value: 0.85, min: 0, max: 1 });
  // Gentle wobble on the upper bound — `hiFinal = hi + sin(t * 0.6) * 0.05`.
  // Keeps the streaks shimmering AND keeps the patch ticking after the image
  // loads (which is async; without a time-varying input the patch settles
  // before the image arrives and the canvas stays black until a slider moves).
  const tMul     = node('ps_tMul',   'math/mul',     { x: -240, y: 100 }, { b: 0.6 });
  const sinT     = node('ps_sinT',   'math/sin',     { x:  -80, y: 100 });
  const wobMul   = node('ps_wobMul', 'math/mul',     { x:  -80, y: 160 }, { b: 0.05 });
  const hiFinal  = node('ps_hiFinal','math/add',     { x:   80, y: 200 });
  const sort     = node('ps_sort',   'pixel/sort', { x: 240, y: 40 },
                        { axis: 'rows', criterion: 'brightness', order: 'ascending' });
  const draw     = node('ps_draw',   'draw/image',   { x: 460, y: 40 },
                        { x: -250, y: -250, w: 500, h: 500 });
  const bg       = node('ps_bg',     'color/rgb',    { x: 460, y: 160 }, { r: 12, g: 10, b: 16, a: 255 });
  const renderer = node('ps_renderer','render/renderer',{ x: 720, y: 20 }, { width: 500, height: 500 });

  return {
    nodes: [img, time, lo, hi, tMul, sinT, wobMul, hiFinal, sort, draw, bg, renderer],
    edges: [
      edge('ps_img',  'image', 'ps_sort',  'image'),
      // hiFinal = hi + sin(time * 0.6) * 0.05
      edge('ps_time', 't',     'ps_tMul',  'a'),
      edge('ps_tMul', 'out',   'ps_sinT',  'x'),
      edge('ps_sinT', 'out',   'ps_wobMul','a'),
      edge('ps_hi',   'out',   'ps_hiFinal','a'),
      edge('ps_wobMul','out',  'ps_hiFinal','b'),
      edge('ps_lo',   'out',   'ps_sort',  'lo'),
      edge('ps_hiFinal','out', 'ps_sort',  'hi'),
      edge('ps_sort', 'image', 'ps_draw',  'image'),
      edge('ps_draw', 'out',   'ps_renderer', 'layers'),
      edge('ps_bg',   'out',   'ps_renderer', 'background'),
    ],
  };
}
