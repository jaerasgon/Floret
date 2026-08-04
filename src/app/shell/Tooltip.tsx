import { useCallback, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

/**
 * Lightweight native tooltip. Keeps Floret's original API (`label` / `side` /
 * `shortcut`) — a hover-delayed, body-portaled hint positioned above or below
 * the wrapped element. Replaces the former Astryx Tooltip so the chrome has no
 * design-system dependency.
 */
export function Tooltip({
  label,
  side = 'bottom',
  shortcut,
  children,
}: {
  label: string;
  side?: 'top' | 'bottom';
  shortcut?: string;
  children: ReactNode;
}) {
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const timer = useRef<number | undefined>(undefined);

  const show = useCallback(
    (e: React.MouseEvent) => {
      const el = e.currentTarget as HTMLElement;
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => {
        const r = el.getBoundingClientRect();
        setPos({ x: r.left + r.width / 2, y: side === 'top' ? r.top - 8 : r.bottom + 8 });
      }, 380);
    },
    [side],
  );
  const hide = useCallback(() => {
    window.clearTimeout(timer.current);
    setPos(null);
  }, []);

  const text = shortcut ? `${label}  ${shortcut}` : label;
  return (
    <span
      className="tt-anchor"
      onMouseEnter={show}
      onMouseLeave={hide}
      onMouseDown={hide}
    >
      {children}
      {pos &&
        createPortal(
          <span className={`tt-pop tt-${side}`} style={{ left: pos.x, top: pos.y }} role="tooltip">
            {text}
          </span>,
          document.body,
        )}
    </span>
  );
}
