import { useCallback, useEffect, useRef, useState } from 'react';

/**
 * Number input you can EITHER type into OR drag horizontally on to
 * scrub the value. Pointer-locked drag (so the cursor doesn't have
 * to leave the input) with the following modifiers:
 *
 *   default        — step
 *   Shift          — 10× step
 *   Alt / Option   —  0.1× step
 *
 * The value is committed live (via `onLive`) while dragging so the
 * canvas updates in real time; on release the parent's `onCommit` is
 * also called so save / undo see the final value.
 */
export interface ScrubInputProps {
  value: number;
  step?: number;
  min?: number;
  max?: number;
  className?: string;
  /** Live-update callback fired on every scrub tick OR keystroke. */
  onLive: (v: number) => void;
  /** Fired once after a scrub ends or on blur — the "value is final" signal. */
  onCommit: () => void;
}

const DRAG_THRESHOLD_PX = 3;
const PX_PER_STEP = 4;

export function ScrubInput({
  value, step = 1, min, max, className, onLive, onCommit,
}: ScrubInputProps) {
  const [draft, setDraft] = useState<string>(formatValue(value));
  const [scrubbing, setScrubbing] = useState(false);
  const dragStateRef = useRef<{
    startX: number;
    startValue: number;
    activated: boolean;
    pointerId: number;
  } | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Keep the input in sync with external value changes (e.g. paste,
  // undo, wire connection). Skip while the user is typing to avoid
  // clobbering an in-progress edit.
  useEffect(() => {
    if (document.activeElement !== inputRef.current && !scrubbing) {
      setDraft(formatValue(value));
    }
  }, [value, scrubbing]);

  const clamp = useCallback(
    (v: number): number => {
      let r = v;
      if (typeof min === 'number') r = Math.max(min, r);
      if (typeof max === 'number') r = Math.min(max, r);
      return r;
    },
    [min, max],
  );

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLInputElement>) => {
      // Don't start scrubbing on right/middle click.
      if (e.button !== 0) return;
      dragStateRef.current = {
        startX: e.clientX,
        startValue: value,
        activated: false,
        pointerId: e.pointerId,
      };
    },
    [value],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLInputElement>) => {
      const drag = dragStateRef.current;
      if (!drag) return;
      const dx = e.clientX - drag.startX;
      if (!drag.activated) {
        if (Math.abs(dx) < DRAG_THRESHOLD_PX) return;
        drag.activated = true;
        setScrubbing(true);
        e.currentTarget.setPointerCapture(drag.pointerId);
        // Take focus away from the text caret so typing doesn't happen.
        e.currentTarget.blur();
      }
      const mod = e.shiftKey ? 10 : e.altKey ? 0.1 : 1;
      const next = clamp(drag.startValue + (dx / PX_PER_STEP) * step * mod);
      setDraft(formatValue(next));
      onLive(next);
    },
    [step, clamp, onLive],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent<HTMLInputElement>) => {
      const drag = dragStateRef.current;
      dragStateRef.current = null;
      if (drag?.activated) {
        try { e.currentTarget.releasePointerCapture(drag.pointerId); } catch { /* noop */ }
        setScrubbing(false);
        onCommit();
      }
    },
    [onCommit],
  );

  const onChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      setDraft(e.target.value);
      const v = Number(e.target.value);
      if (Number.isFinite(v)) onLive(clamp(v));
    },
    [onLive, clamp],
  );

  const onBlur = useCallback(() => {
    const v = Number(draft);
    if (!Number.isFinite(v)) {
      setDraft(formatValue(value));
      return;
    }
    onCommit();
  }, [draft, value, onCommit]);

  return (
    <input
      ref={inputRef}
      type="text"
      inputMode="decimal"
      className={`${className ?? ''} scrub-input${scrubbing ? ' scrubbing' : ''}`}
      value={draft}
      onChange={onChange}
      onBlur={onBlur}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
    />
  );
}

function formatValue(v: number): string {
  if (Number.isInteger(v)) return String(v);
  return Number(v.toFixed(3)).toString();
}
