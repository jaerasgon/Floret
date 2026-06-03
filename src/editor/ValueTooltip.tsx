import { useEffect, useRef, useState } from 'react';
import { getRuntimeValue } from '../runtime/inspector';

/**
 * Hover-shown tooltip displaying the live runtime value at a node's
 * output port. Polls the `inspector` module via requestAnimationFrame
 * while open (cheap — only one tooltip is ever visible at a time).
 */
export interface ValueTooltipProps {
  nodeId: string;
  portIdx: number;
  portName: string;
  portType: string;
  /** True when this port carries a Spread<T>. Shown as a "· spread"
   *  suffix on the type chip — pin shape itself stays a round circle. */
  isSpread?: boolean;
  anchor: { x: number; y: number };
  onClose: () => void;
}

export function ValueTooltip({ nodeId, portIdx, portName, portType, isSpread, anchor }: ValueTooltipProps) {
  const [text, setText] = useState(() => formatValue(getRuntimeValue(nodeId, portIdx)));
  const rafRef = useRef<number | null>(null);

  useEffect(() => {
    let mounted = true;
    function tick() {
      if (!mounted) return;
      setText(formatValue(getRuntimeValue(nodeId, portIdx)));
      rafRef.current = requestAnimationFrame(tick);
    }
    rafRef.current = requestAnimationFrame(tick);
    return () => {
      mounted = false;
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
    };
  }, [nodeId, portIdx]);

  return (
    <div
      className="value-tooltip"
      style={{ left: anchor.x, top: anchor.y }}
    >
      <div className="value-tooltip-header">
        <span className="value-tooltip-port">{portName}</span>
        <span className="value-tooltip-type">
          {portType}{isSpread && <span className="value-tooltip-spread"> · spread</span>}
        </span>
      </div>
      <div className="value-tooltip-body">{text}</div>
    </div>
  );
}

/**
 * Format a runtime value for tooltip display. Spreads show just their
 * first element along with a `(1 of N)` count badge so wide fan-outs
 * (and inner-region per-iteration aggregations) stay readable; objects
 * (vec2, color, mat3) get a compact key:value preview; functions render
 * as `[fn]`.
 */
function formatValue(v: unknown): string {
  return formatInner(v, true);
}

function formatInner(v: unknown, topLevel: boolean): string {
  if (v === undefined) return '—';
  if (v === null) return 'null';
  if (typeof v === 'number') {
    return Number.isInteger(v) ? String(v) : v.toFixed(3).replace(/\.?0+$/, '');
  }
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (typeof v === 'string') return `"${v}"`;
  if (typeof v === 'function') return '[fn]';
  if (Array.isArray(v)) {
    if (v.length === 0) return '[ ]';
    // At the top level, surface just the first element with a count
    // badge — easier to read than a 4-element preview, and gives the
    // user a clear sense that this is the head of a wider stream.
    // Nested arrays still bracket so structure is visible.
    if (topLevel) {
      const head = formatInner(v[0], false);
      return v.length > 1 ? `${head}  (1 of ${v.length})` : head;
    }
    const headN = v.slice(0, 3).map((x) => formatInner(x, false)).join(', ');
    return v.length > 3 ? `[${headN}, …×${v.length}]` : `[${headN}]`;
  }
  if (typeof v === 'object') {
    const o = v as Record<string, unknown>;
    if (typeof o.x === 'number' && typeof o.y === 'number' && Object.keys(o).length === 2) {
      return `vec2(${formatInner(o.x, false)}, ${formatInner(o.y, false)})`;
    }
    if (typeof o.r === 'number' && typeof o.g === 'number' && typeof o.b === 'number') {
      const hx = (n: number) => Math.max(0, Math.min(255, Math.round(n * 255))).toString(16).padStart(2, '0');
      return `#${hx(o.r)}${hx(o.g)}${hx(o.b)}`;
    }
    try { return JSON.stringify(v); } catch { return '[object]'; }
  }
  return String(v);
}
