import type { ReactNode } from 'react';
import { Tooltip as AstryxTooltip } from '@astryxdesign/core/Tooltip';

/**
 * Thin adapter that keeps Floret's original Tooltip API (`label` / `side` /
 * `shortcut`) while delegating rendering to the Astryx design-system Tooltip.
 * Existing call sites don't change; they just get Astryx's accessible,
 * portal-positioned tooltip instead of the old CSS-attribute hover hint.
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
  const text = shortcut ? `${label}  ${shortcut}` : label;
  return (
    <AstryxTooltip content={text} placement={side === 'top' ? 'above' : 'below'}>
      {children}
    </AstryxTooltip>
  );
}
