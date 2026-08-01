/**
 * Floret brand mark — a small single-colour geometric sunflower (13 petals +
 * seed head) drawn as inline SVG. Pinned to a fixed sunflower-gold so it stays
 * recognisably a sunflower against the neutral (grey-accent) theme in both
 * light and dark. Replaces the old "FLORET" wordmark.
 */
const GOLD = '#F4C542';

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
      fill={GOLD}
      style={{ display: 'block' }}
    >
      <title>Floret</title>
      {petals}
      <circle cx="12" cy="12" r="4" />
    </svg>
  );
}
