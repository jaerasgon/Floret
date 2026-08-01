import type { ReactElement } from 'react';
import MuiTooltip from '@mui/material/Tooltip';

/**
 * Thin adapter that keeps Floret's original Tooltip API (`label` / `side` /
 * `shortcut`) while delegating rendering to the MUI (Material 3) Tooltip.
 * Existing call sites don't change; they just get MUI's accessible,
 * portal-positioned tooltip.
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
  children: ReactElement;
}) {
  const text = shortcut ? `${label}  ${shortcut}` : label;
  return (
    <MuiTooltip title={text} placement={side === 'top' ? 'top' : 'bottom'} arrow>
      {children}
    </MuiTooltip>
  );
}
