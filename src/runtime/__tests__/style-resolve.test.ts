import { describe, expect, it } from 'vitest';
import { resolveStyle, DEFAULT_STYLE, LINE_DEFAULT_STYLE, type Style } from '../scene';

// Minimal Style functions mirroring what the style nodes emit. Each only
// sets its own field(s) and otherwise passes the base through — exactly
// like style/fill and style/stroke.
const stroke: Style = (s) => ({ ...s, stroke: '#000000', strokeWeight: 2 });
const fill: Style = (s) => ({ ...s, fill: '#ff0000' });

describe('resolveStyle — a wired style replaces the shape default', () => {
  it('no style wired → falls back to the shape default (white fill)', () => {
    expect(resolveStyle(null, DEFAULT_STYLE).fill).toBe('#ffffff');
    expect(resolveStyle(undefined, LINE_DEFAULT_STYLE).stroke).toBe('#000000');
  });

  it('stroke-only style → outline only, the default fill does NOT leak through', () => {
    const r = resolveStyle(stroke, DEFAULT_STYLE);
    expect(r.fill).toBeNull();
    expect(r.stroke).toBe('#000000');
    expect(r.strokeWeight).toBe(2);
  });

  it('fill-only style → fill set, no stroke', () => {
    const r = resolveStyle(fill, DEFAULT_STYLE);
    expect(r.fill).toBe('#ff0000');
    expect(r.stroke).toBeNull();
  });

  it('Fill → Stroke chain → both fill and stroke', () => {
    const chained: Style = (s) => stroke(fill(s));
    const r = resolveStyle(chained, DEFAULT_STYLE);
    expect(r.fill).toBe('#ff0000');
    expect(r.stroke).toBe('#000000');
  });

  it('wiring a style into a line replaces its default black stroke too', () => {
    // Fill-only into a line: replaces the default → fill set, stroke gone.
    const r = resolveStyle(fill, LINE_DEFAULT_STYLE);
    expect(r.fill).toBe('#ff0000');
    expect(r.stroke).toBeNull();
  });

  it('keeps non-paint structural defaults (strokeWeight base, fontSize, modes)', () => {
    const r = resolveStyle(fill, DEFAULT_STYLE);
    expect(r.strokeWeight).toBe(DEFAULT_STYLE.strokeWeight);
    expect(r.fontSize).toBe(DEFAULT_STYLE.fontSize);
    expect(r.rectMode).toBe(DEFAULT_STYLE.rectMode);
    expect(r.ellipseMode).toBe(DEFAULT_STYLE.ellipseMode);
  });
});
