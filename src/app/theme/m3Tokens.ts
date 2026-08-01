/**
 * Material 3 colour roles, generated once from the seed colour #F4C542
 * (Floret amber) with @material/material-color-utilities' `themeFromSourceColor`.
 * Surface-container tones are taken from the neutral tonal palette per the M3
 * spec. Baked as constants so there is no runtime dependency on the generator
 * (the roles are deterministic for a fixed seed).
 *
 * To regenerate for a different seed, feed the seed through
 * material-color-utilities and replace both maps.
 */
export type M3Roles = {
  primary: string; onPrimary: string; primaryContainer: string; onPrimaryContainer: string;
  secondary: string; onSecondary: string; secondaryContainer: string; onSecondaryContainer: string;
  tertiary: string; onTertiary: string; tertiaryContainer: string; onTertiaryContainer: string;
  error: string; onError: string; errorContainer: string; onErrorContainer: string;
  background: string; onBackground: string;
  surface: string; onSurface: string; surfaceVariant: string; onSurfaceVariant: string;
  outline: string; outlineVariant: string;
  inverseSurface: string; inverseOnSurface: string; inversePrimary: string;
  shadow: string; scrim: string;
  surfaceDim: string; surfaceBright: string;
  surfaceContainerLowest: string; surfaceContainerLow: string; surfaceContainer: string;
  surfaceContainerHigh: string; surfaceContainerHighest: string;
};

export const M3_SEED = '#F4C542';

export const m3Light: M3Roles = {
  primary: '#765b00', onPrimary: '#ffffff', primaryContainer: '#ffdf94', onPrimaryContainer: '#241a00',
  secondary: '#6a5d3f', onSecondary: '#ffffff', secondaryContainer: '#f3e1bb', onSecondaryContainer: '#231a04',
  tertiary: '#486549', onTertiary: '#ffffff', tertiaryContainer: '#caebc7', onTertiaryContainer: '#05210a',
  error: '#ba1a1a', onError: '#ffffff', errorContainer: '#ffdad6', onErrorContainer: '#410002',
  background: '#fffbff', onBackground: '#1e1b16',
  surface: '#fffbff', onSurface: '#1e1b16', surfaceVariant: '#ece1cf', onSurfaceVariant: '#4c4639',
  outline: '#7e7667', outlineVariant: '#cfc5b4',
  inverseSurface: '#33302a', inverseOnSurface: '#f7f0e7', inversePrimary: '#efc13e',
  shadow: '#000000', scrim: '#000000',
  surfaceDim: '#e0d9d0', surfaceBright: '#fff8f1',
  surfaceContainerLowest: '#ffffff', surfaceContainerLow: '#faf2e9', surfaceContainer: '#f4ede4',
  surfaceContainerHigh: '#eee7de', surfaceContainerHighest: '#e9e1d9',
};

export const m3Dark: M3Roles = {
  primary: '#efc13e', onPrimary: '#3e2e00', primaryContainer: '#594400', onPrimaryContainer: '#ffdf94',
  secondary: '#d6c5a0', onSecondary: '#392f15', secondaryContainer: '#51462a', onSecondaryContainer: '#f3e1bb',
  tertiary: '#aecfac', onTertiary: '#1b361e', tertiaryContainer: '#314d33', onTertiaryContainer: '#caebc7',
  error: '#ffb4ab', onError: '#690005', errorContainer: '#93000a', onErrorContainer: '#ffb4ab',
  background: '#1e1b16', onBackground: '#e9e1d9',
  surface: '#1e1b16', onSurface: '#e9e1d9', surfaceVariant: '#4c4639', onSurfaceVariant: '#cfc5b4',
  outline: '#989080', outlineVariant: '#4c4639',
  inverseSurface: '#e9e1d9', inverseOnSurface: '#33302a', inversePrimary: '#765b00',
  shadow: '#000000', scrim: '#000000',
  surfaceDim: '#16130e', surfaceBright: '#3c3933',
  surfaceContainerLowest: '#100e09', surfaceContainerLow: '#1e1b16', surfaceContainer: '#221f1a',
  surfaceContainerHigh: '#2d2a24', surfaceContainerHighest: '#38342e',
};

/** `--md-sys-color-<role>` CSS custom-property name for an M3 role key. */
export function mdVarName(role: keyof M3Roles): string {
  return '--md-sys-color-' + role.replace(/[A-Z]/g, (m) => '-' + m.toLowerCase());
}

/** Build the `{ '--md-sys-color-*': value }` map for one scheme. */
export function mdVars(roles: M3Roles): Record<string, string> {
  const out: Record<string, string> = {};
  (Object.keys(roles) as (keyof M3Roles)[]).forEach((k) => { out[mdVarName(k)] = roles[k]; });
  return out;
}
