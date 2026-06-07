import { cloneElement, isValidElement } from 'react';
import type { ReactElement, ReactNode } from 'react';

/**
 * Lightweight wrapper that attaches a `data-tooltip` attribute to its child.
 * Rendering happens entirely in CSS via the `[data-tooltip]:hover::after`
 * rules in styles.css — no JS state, no portals, no measurement.
 *
 * Use side="bottom" for toolbar buttons (tooltip flips down so it doesn't
 * clip behind the toolbar's bottom border).
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
  if (!isValidElement(children)) return <>{children}</>;
  const text = shortcut ? `${label}  ${shortcut}` : label;
  return cloneElement(children as ReactElement<Record<string, unknown>>, {
    'data-tooltip': text,
    'data-tooltip-side': side,
  });
}
