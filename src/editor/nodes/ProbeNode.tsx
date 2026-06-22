import { memo, useEffect, useRef, useState } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { type FloretNodeData } from '../editor-state';
import { nodeRegistry } from '../../nodes/registry';
import { getLatestOutputs } from '../../runtime/inspector';
import { renderScene } from '../../runtime/scene-renderer';
import { isReadyImageCanvas } from '../../nodes/_shared/coerce';
import type { SceneNode } from '../../runtime/scene';

/**
 * Probe node — a persistent readout of the data flowing through it, plus an
 * input + output pin so it reroutes the wire. Works for every data type:
 *
 *   - scalars / vec2 / color / arrays → a scrollable index→value list
 *   - `image` (a canvas, e.g. the camera) → a fitted thumbnail
 *   - `layer` (SceneNodes) → a rendered preview of the drawing
 *
 * Reads this node's own live output (`out`, port 0) from the inspector on a
 * throttled poll.
 */
const POLL_MS = 120;
const MAX_ROWS = 400;
const THUMB_W = 132;
const THUMB_H = 100;
/** Canvas size scenes are typically authored against (centre origin). */
const LAYER_BASE = 520;

type Mode = 'text' | 'image' | 'layer';

interface Snapshot {
  count: number;          // -1 = no data yet
  type: string;
  rows: string[];
  single: string | null;
}
const EMPTY: Snapshot = { count: -1, type: '', rows: [], single: null };

export const ProbeNode = memo(function ProbeNode(props: NodeProps) {
  const { id, data, selected } = props;
  const nodeData = data as unknown as FloretNodeData;
  const def = nodeRegistry.get(nodeData.defType);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [mode, setMode] = useState<Mode>('text');
  const [snap, setSnap] = useState<Snapshot>(EMPTY);

  useEffect(() => {
    let mounted = true;
    const read = () => {
      if (!mounted) return;
      const port = getLatestOutputs().get(id)?.[0];
      const first = port && port.length ? port[0] : undefined;
      if (isReadyImageCanvas(first)) {
        setMode('image');
        drawImageThumb(canvasRef.current, first as unknown as CanvasImageSource & { width: number; height: number });
      } else if (isSceneNode(first)) {
        setMode('layer');
        drawLayerThumb(canvasRef.current, port as ReadonlyArray<SceneNode>);
      } else {
        setMode('text');
        setSnap(snapshot(port));
      }
    };
    read();
    const t = window.setInterval(read, POLL_MS);
    return () => { mounted = false; window.clearInterval(t); };
  }, [id]);

  if (!def) return null;

  const visual = mode === 'image' || mode === 'layer';
  const isSpread = snap.count > 1;
  const hidden = isSpread ? Math.max(0, snap.count - snap.rows.length) : 0;

  return (
    <div className="node node-probe" data-selected={selected ? 1 : 0} data-category="meta">
      <Handle
        id="in-in" type="target" position={Position.Left}
        className="node-row-handle node-row-handle-left node-probe-pin"
        style={{ background: 'var(--pin-spread)', borderColor: 'var(--pin-spread)' }}
      />
      <Handle
        id="out-out" type="source" position={Position.Right}
        className="node-row-handle node-row-handle-right node-probe-pin"
        style={{ background: 'var(--pin-spread)', borderColor: 'var(--pin-spread)' }}
      />

      <div className="node-probe-header">
        <span className="node-probe-label">probe</span>
        <span className="node-probe-meta">
          {visual ? mode
            : snap.count < 0 ? '—'
            : snap.count === 0 ? 'empty'
            : isSpread ? `${snap.type} · ${snap.count}` : snap.type}
        </span>
      </div>

      {visual ? (
        <div className="node-probe-preview nodrag">
          <canvas ref={canvasRef} width={THUMB_W} height={THUMB_H} className="node-probe-canvas" />
        </div>
      ) : (
        <div className="node-probe-body nodrag nowheel">
          {snap.count < 0 && <div className="node-probe-line node-probe-dim">no data (run the patch)</div>}
          {snap.count === 0 && <div className="node-probe-line node-probe-dim">[ ] empty</div>}
          {snap.count === 1 && <div className="node-probe-line">{snap.single}</div>}
          {isSpread && snap.rows.map((r, i) => (
            <div key={i} className="node-probe-line">
              <span className="node-probe-idx">{i}</span>
              <span className="node-probe-val">{r}</span>
            </div>
          ))}
          {hidden > 0 && <div className="node-probe-line node-probe-dim">+{hidden} more…</div>}
        </div>
      )}
    </div>
  );
});

// ─── previews ───────────────────────────────────────────────────────────

function isSceneNode(v: unknown): v is SceneNode {
  return !!v && typeof v === 'object' && typeof (v as { kind?: unknown }).kind === 'string';
}

function clearThumb(canvas: HTMLCanvasElement): CanvasRenderingContext2D | null {
  const ctx = canvas.getContext('2d');
  if (!ctx) return null;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = '#0d0f14';
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  return ctx;
}

function drawImageThumb(canvas: HTMLCanvasElement | null, src: CanvasImageSource & { width: number; height: number }): void {
  if (!canvas) return;
  const ctx = clearThumb(canvas);
  if (!ctx) return;
  const sw = src.width, sh = src.height;
  if (!sw || !sh) return;
  const scale = Math.min(canvas.width / sw, canvas.height / sh);
  const w = sw * scale, h = sh * scale;
  try { ctx.drawImage(src, (canvas.width - w) / 2, (canvas.height - h) / 2, w, h); } catch { /* tainted */ }
}

function drawLayerThumb(canvas: HTMLCanvasElement | null, items: ReadonlyArray<SceneNode>): void {
  if (!canvas) return;
  const ctx = clearThumb(canvas);
  if (!ctx) return;
  const s = Math.min(canvas.width, canvas.height) / LAYER_BASE;
  ctx.save();
  ctx.translate(canvas.width / 2, canvas.height / 2);
  ctx.scale(s, s);
  try {
    for (const item of items) if (isSceneNode(item)) renderScene(ctx, item);
  } catch { /* ignore malformed scene */ }
  ctx.restore();
}

// ─── text snapshot + formatting ───────────────────────────────────────────

function snapshot(port: ReadonlyArray<unknown> | undefined): Snapshot {
  if (port === undefined) return EMPTY;
  const n = port.length;
  if (n === 0) return { count: 0, type: '', rows: [], single: null };
  const type = inferType(port[0]);
  if (n === 1) return { count: 1, type, rows: [], single: fmt(port[0]) };
  const rows = port.slice(0, MAX_ROWS).map(fmt);
  return { count: n, type, rows, single: null };
}

function inferType(v: unknown): string {
  if (v === null || v === undefined) return 'null';
  if (typeof v === 'number') return 'number';
  if (typeof v === 'boolean') return 'boolean';
  if (typeof v === 'string') return 'string';
  if (typeof v === 'function') return 'fn';
  if (Array.isArray(v)) return 'array';
  if (typeof v === 'object') {
    const o = v as Record<string, unknown>;
    if (typeof o.x === 'number' && typeof o.y === 'number' && Object.keys(o).length === 2) return 'vec2';
    if (typeof o.r === 'number' && typeof o.g === 'number' && typeof o.b === 'number') return 'color';
    return 'object';
  }
  return typeof v;
}

/** Compact single-line format for one element. */
function fmt(v: unknown): string {
  if (v === undefined) return '—';
  if (v === null) return 'null';
  if (typeof v === 'number') return Number.isInteger(v) ? String(v) : v.toFixed(3).replace(/\.?0+$/, '');
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (typeof v === 'string') return `"${v}"`;
  if (typeof v === 'function') return '[fn]';
  if (Array.isArray(v)) {
    const head = v.slice(0, 4).map(fmt).join(', ');
    return v.length > 4 ? `[${head}, …×${v.length}]` : `[${head}]`;
  }
  if (typeof v === 'object') {
    const o = v as Record<string, unknown>;
    if (typeof o.kind === 'string') return `‹${o.kind}›`;
    if (typeof o.x === 'number' && typeof o.y === 'number' && Object.keys(o).length === 2) {
      return `(${fmt(o.x)}, ${fmt(o.y)})`;
    }
    if (typeof o.r === 'number' && typeof o.g === 'number' && typeof o.b === 'number') {
      const hx = (k: number) => Math.max(0, Math.min(255, Math.round(k * 255))).toString(16).padStart(2, '0');
      return `#${hx(o.r)}${hx(o.g)}${hx(o.b)}`;
    }
    try { return JSON.stringify(v); } catch { return '[object]'; }
  }
  return String(v);
}
