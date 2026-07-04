import { useEffect, useRef } from 'react';
import { useEditor } from './editor-state';

/**
 * Ambient "living canvas" — a faint generative flow-field painted behind the
 * node grid so an empty (or quiet) patch never feels dead. It's a small nod to
 * what Floret makes: a slow drift of ink following a noise field.
 *
 * Design constraints:
 *   - Screen-space, not graph-space — it reads as a backdrop, not content, so
 *     it doesn't pan/zoom with the graph.
 *   - Opaque (primed with --bg-0) so it *is* the canvas background; React Flow
 *     is made transparent on top of it (see styles.css). The dot grid + nodes
 *     render above.
 *   - Fades down as the patch fills up (opacity driven by node count) so it
 *     never competes with a busy graph.
 *   - Honours prefers-reduced-motion: lays down one static field, no rAF.
 */
export function AmbientFlow() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const nodeCount = useEditor((s) => s.nodes.length);

  // Busy patch → quieter backdrop. Empty → full presence.
  useEffect(() => {
    const el = canvasRef.current;
    if (!el) return;
    const t = Math.max(0, Math.min(1, (nodeCount - 3) / 26));
    el.style.opacity = String(0.85 - t * 0.68); // ~0.85 empty → ~0.17 busy
  }, [nodeCount]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const parent = canvas?.parentElement;
    const ctx = canvas?.getContext('2d');
    if (!canvas || !parent || !ctx) return;

    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);

    // Colours track the theme (read from CSS vars; re-read on theme flip).
    let bg = '#100f0d';
    let ink = '#f4c542';
    const readColors = () => {
      const cs = getComputedStyle(document.documentElement);
      bg = cs.getPropertyValue('--bg-0').trim() || bg;
      ink = cs.getPropertyValue('--accent').trim() || ink;
    };
    readColors();
    const themeObs = new MutationObserver(() => { readColors(); prime(); });
    themeObs.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });

    let w = 0;
    let h = 0;
    const prime = () => { ctx.fillStyle = bg; ctx.fillRect(0, 0, w, h); };
    const resize = () => {
      const r = parent.getBoundingClientRect();
      w = r.width; h = r.height;
      canvas.width = Math.max(1, Math.floor(w * dpr));
      canvas.height = Math.max(1, Math.floor(h * dpr));
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      prime();
    };
    const ro = new ResizeObserver(resize);
    ro.observe(parent);
    resize();

    const N = 150;
    const parts = Array.from({ length: N }, () => ({
      x: Math.random() * w,
      y: Math.random() * h,
      life: Math.random() * 220,
    }));
    const TAU = Math.PI * 2;
    // Cheap layered-sine flow field — no noise dependency, plenty organic at
    // this scale/opacity.
    const field = (x: number, y: number, t: number) =>
      TAU * (Math.sin(x * 0.0009 + t * 0.00022) +
             Math.cos(y * 0.0011 - t * 0.00016) +
             Math.sin((x + y) * 0.0006 + t * 0.00030));

    const step = (t: number) => {
      // Fade the previous frame toward bg → soft dissipating trails.
      ctx.globalCompositeOperation = 'source-over';
      ctx.globalAlpha = 0.055;
      ctx.fillStyle = bg;
      ctx.fillRect(0, 0, w, h);

      ctx.globalAlpha = 0.07;
      ctx.strokeStyle = ink;
      ctx.lineWidth = 1.1;
      for (const p of parts) {
        const a = field(p.x, p.y, t);
        const nx = p.x + Math.cos(a) * 1.15;
        const ny = p.y + Math.sin(a) * 1.15;
        ctx.beginPath();
        ctx.moveTo(p.x, p.y);
        ctx.lineTo(nx, ny);
        ctx.stroke();
        p.x = nx; p.y = ny; p.life -= 1;
        if (p.life < 0 || p.x < -2 || p.x > w + 2 || p.y < -2 || p.y > h + 2) {
          p.x = Math.random() * w;
          p.y = Math.random() * h;
          p.life = 120 + Math.random() * 220;
        }
      }
      ctx.globalAlpha = 1;
    };

    let raf = 0;
    if (reduce) {
      // Static field: run a fixed number of steps to lay down streaks, then stop.
      for (let i = 0; i < 260; i++) step(i * 16);
    } else {
      const loop = (t: number) => { step(t); raf = requestAnimationFrame(loop); };
      raf = requestAnimationFrame(loop);
    }

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      themeObs.disconnect();
    };
  }, []);

  return <canvas ref={canvasRef} className="ambient-flow" aria-hidden="true" />;
}
