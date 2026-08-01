/**
 * Floret brand mark — a small single-colour geometric sunflower. Everything
 * (petals + seed head) is filled with one colour, the theme's amber primary,
 * so it reads as a flat monochrome icon that stays visible in both light and
 * dark. Replaces the old "FLORET" wordmark.
 */
export function FloretLogo({ size = 22 }: { size?: number }) {
  const PETALS = 13;
  const petals = Array.from({ length: PETALS }, (_, i) => {
    const angle = (i * 360) / PETALS;
    return (
      <ellipse
        key={i}
        cx="12"
        cy="4.3"
        rx="1.5"
        ry="3.5"
        transform={`rotate(${angle} 12 12)`}
      />
    );
  });
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      role="img"
      aria-label="Floret"
      fill="var(--md-sys-color-primary)"
      style={{ display: 'block' }}
    >
      <title>Floret</title>
      {petals}
      {/* Seed head — a small gap ring keeps it distinct from the petals while
          staying a single colour. */}
      <circle cx="12" cy="12" r="4" />
    </svg>
  );
}
