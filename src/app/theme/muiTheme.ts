import { createTheme, type Theme } from '@mui/material/styles';
import { m3Light, m3Dark, type M3Roles } from './m3Tokens';

/**
 * Builds a MUI theme from the Material 3 role set for the given mode. We do not
 * use MUI's built-in CSS-variable colour-scheme switching: Floret already owns
 * the light/dark preference (persisted in its prefs store and reflected as
 * `data-theme` on <html>), so the single source of truth stays there and this
 * theme is simply recomputed when the mode changes. Component colours therefore
 * resolve to concrete M3 role values rather than to a second scheme manager.
 *
 * The palette maps M3 colour roles onto MUI's slots; component overrides push
 * the rest of the Material 3 look — Roboto, fully-rounded buttons, tonal
 * surface-container fills for menus/dialogs, flat elevation (no dark overlay),
 * and 28px dialog / pill shapes.
 */
const ROBOTO = '"Roboto", "Helvetica", "Arial", sans-serif';

export function makeM3Theme(mode: 'light' | 'dark'): Theme {
  const c: M3Roles = mode === 'dark' ? m3Dark : m3Light;

  return createTheme({
    palette: {
      mode,
      primary: { main: c.primary, contrastText: c.onPrimary },
      secondary: { main: c.secondary, contrastText: c.onSecondary },
      error: { main: c.error, contrastText: c.onError },
      // M3 has no info/success/warning; approximate from the tertiary/secondary
      // families so semantic MUI components still read on-theme.
      success: { main: c.tertiary, contrastText: c.onTertiary },
      warning: { main: c.secondary, contrastText: c.onSecondary },
      info: { main: c.tertiary, contrastText: c.onTertiary },
      background: { default: c.background, paper: c.surfaceContainerLow },
      text: { primary: c.onSurface, secondary: c.onSurfaceVariant, disabled: c.outline },
      divider: c.outlineVariant,
      action: {
        active: c.onSurfaceVariant,
        hover: withAlpha(c.onSurface, 0.08),
        selected: withAlpha(c.primary, 0.16),
        disabled: withAlpha(c.onSurface, 0.38),
        disabledBackground: withAlpha(c.onSurface, 0.12),
        focus: withAlpha(c.primary, 0.12),
      },
    },
    shape: { borderRadius: 12 }, // M3 "medium" corner
    typography: {
      fontFamily: ROBOTO,
      button: { textTransform: 'none', fontWeight: 500, letterSpacing: 0 },
    },
    components: {
      // Kill MUI's dark-mode elevation overlay so surfaces stay flat M3 tones.
      MuiPaper: {
        styleOverrides: {
          root: { backgroundImage: 'none' },
        },
      },
      MuiButton: {
        defaultProps: { disableElevation: true },
        styleOverrides: {
          // Pill shape, but no blanket padding — let the size variants below
          // set density so compact chrome (toolbar / panel headers) doesn't
          // inherit dialog-sized buttons. minWidth:0 stops MUI's 64px default
          // from spreading toolbar buttons apart.
          root: { borderRadius: 9999, minWidth: 0 },
          sizeSmall: { padding: '3px 12px', fontSize: 13, lineHeight: 1.6 },
          sizeMedium: { padding: '6px 22px' },
        },
      },
      MuiIconButton: {
        styleOverrides: {
          root: { borderRadius: 9999 },
          sizeSmall: { padding: 5 },
        },
      },
      MuiToggleButton: {
        styleOverrides: {
          root: {
            borderRadius: 9999,
            textTransform: 'none',
            borderColor: c.outlineVariant,
            color: c.onSurfaceVariant,
            '&.Mui-selected': {
              backgroundColor: c.secondaryContainer,
              color: c.onSecondaryContainer,
              '&:hover': { backgroundColor: c.secondaryContainer },
            },
          },
          sizeSmall: { padding: '5px 8px' },
        },
      },
      MuiDialog: {
        styleOverrides: {
          paper: { backgroundColor: c.surfaceContainerHigh, borderRadius: 28, backgroundImage: 'none' },
        },
      },
      MuiMenu: {
        styleOverrides: {
          paper: { backgroundColor: c.surfaceContainer, borderRadius: 8, backgroundImage: 'none' },
        },
      },
      MuiPopover: {
        styleOverrides: {
          paper: { backgroundColor: c.surfaceContainer, backgroundImage: 'none' },
        },
      },
      MuiTooltip: {
        styleOverrides: {
          tooltip: {
            backgroundColor: c.inverseSurface,
            color: c.inverseOnSurface,
            fontSize: 12,
            borderRadius: 6,
          },
          arrow: { color: c.inverseSurface },
        },
      },
      MuiSwitch: {
        styleOverrides: {
          root: { padding: 8 },
          switchBase: { '&.Mui-checked': { color: c.onPrimary } },
        },
      },
      MuiOutlinedInput: {
        styleOverrides: {
          root: {
            borderRadius: 8,
            '& .MuiOutlinedInput-notchedOutline': { borderColor: c.outlineVariant },
            '&:hover .MuiOutlinedInput-notchedOutline': { borderColor: c.onSurface },
            '&.Mui-focused .MuiOutlinedInput-notchedOutline': { borderColor: c.primary, borderWidth: 2 },
            '& .MuiInputBase-inputSizeSmall': { paddingTop: 7, paddingBottom: 7 },
          },
        },
      },
      MuiChip: {
        styleOverrides: {
          root: { borderRadius: 8 },
        },
      },
    },
  });
}

/** #rrggbb + 0..1 alpha → #rrggbbaa. */
function withAlpha(hex: string, alpha: number): string {
  const a = Math.round(Math.max(0, Math.min(1, alpha)) * 255)
    .toString(16)
    .padStart(2, '0');
  return hex + a;
}
