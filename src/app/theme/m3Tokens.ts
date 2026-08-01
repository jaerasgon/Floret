/**
 * Material 3 colour roles for Floret. The chromatic roles (primary / secondary
 * / tertiary / error and their containers) were generated from the seed colour
 * #F4C542 (Floret amber) with @material/material-color-utilities. The surface,
 * background, outline and inverse roles have been hand-neutralised to a clean
 * warm-grey ramp: a chromatic seed otherwise tints every surface brown, which
 * reads muddy — neutral surfaces + an amber accent are crisper. Baked as
 * constants (deterministic, no runtime generator dependency).
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
  background: '#fbfbfa', onBackground: '#1c1b1a',
  surface: '#fbfbfa', onSurface: '#1c1b1a', surfaceVariant: '#e4e2dd', onSurfaceVariant: '#47463f',
  outline: '#7a7871', outlineVariant: '#cbc9c3',
  inverseSurface: '#313030', inverseOnSurface: '#f3f1ee', inversePrimary: '#efc13e',
  shadow: '#000000', scrim: '#000000',
  surfaceDim: '#dcdad6', surfaceBright: '#fbfbfa',
  surfaceContainerLowest: '#ffffff', surfaceContainerLow: '#f5f4f2', surfaceContainer: '#efeeeb',
  surfaceContainerHigh: '#e9e8e5', surfaceContainerHighest: '#e3e2df',
};

export const m3Dark: M3Roles = {
  primary: '#efc13e', onPrimary: '#3e2e00', primaryContainer: '#594400', onPrimaryContainer: '#ffdf94',
  secondary: '#d6c5a0', onSecondary: '#392f15', secondaryContainer: '#51462a', onSecondaryContainer: '#f3e1bb',
  tertiary: '#aecfac', onTertiary: '#1b361e', tertiaryContainer: '#314d33', onTertiaryContainer: '#caebc7',
  error: '#ffb4ab', onError: '#690005', errorContainer: '#93000a', onErrorContainer: '#ffb4ab',
  background: '#131312', onBackground: '#e7e5e1',
  surface: '#131312', onSurface: '#e9e7e3', surfaceVariant: '#454440', onSurfaceVariant: '#c9c6c0',
  outline: '#928f88', outlineVariant: '#403f3c',
  inverseSurface: '#e9e7e3', inverseOnSurface: '#313030', inversePrimary: '#765b00',
  shadow: '#000000', scrim: '#000000',
  surfaceDim: '#131312', surfaceBright: '#3a3936',
  surfaceContainerLowest: '#0e0e0d', surfaceContainerLow: '#1b1b19', surfaceContainer: '#1f1f1d',
  surfaceContainerHigh: '#2a2926', surfaceContainerHighest: '#353431',
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
