import { describe, expect, it } from 'vitest';
import { sceneToSvg } from '../scene-svg';
import {
  DEFAULT_STYLE,
  LINE_DEFAULT_STYLE,
  type EllipseShape,
  type GroupNode,
  type LineShape,
  type PolygonShape,
  type RectShape,
  type SceneNode,
  type TextShape,
  type TriangleShape,
} from '../scene';

/**
 * The visitor is a pure function from `SceneNode[]` to a string, so we
 * test it the obvious way: build a small tree, ask for SVG, assert on
 * the substring(s) that prove the right XML came out. We deliberately
 * don't snapshot whole documents — those break on every cosmetic tweak
 * and stop catching real regressions.
 */

const STYLE_RED = { ...DEFAULT_STYLE, fill: '#ff0000' };
const STYLE_RED_STROKE = { ...DEFAULT_STYLE, fill: null, stroke: '#0000ff', strokeWeight: 3 };

function rect(overrides: Partial<RectShape> = {}): RectShape {
  return {
    kind: 'rect',
    x: 0, y: 0, w: 10, h: 20,
    mode: 'corner',
    style: STYLE_RED,
    ...overrides,
  };
}

describe('sceneToSvg', () => {
  describe('document wrapper', () => {
    it('emits an <svg> with viewBox and dimensions', () => {
      const out = sceneToSvg([], { width: 320, height: 240 });
      expect(out).toContain('<svg');
      expect(out).toContain('viewBox="0 0 320 240"');
      expect(out).toContain('width="320"');
      expect(out).toContain('height="240"');
      expect(out).toContain('xmlns="http://www.w3.org/2000/svg"');
      expect(out).toMatch(/<\/svg>$/);
    });

    it('paints a background rect when one is supplied', () => {
      const out = sceneToSvg([], { width: 100, height: 100, background: '#abcdef' });
      expect(out).toContain('<rect width="100" height="100" fill="#abcdef"');
    });

    it('omits the background rect when none is supplied', () => {
      const out = sceneToSvg([], { width: 100, height: 100 });
      // No rect should appear in an empty document. (Layer rects show up
      // with explicit x/y/width/height; the background rect uses width
      // and height alone.)
      expect(out).not.toContain('<rect ');
    });

    it('clamps non-positive dimensions to 1', () => {
      const out = sceneToSvg([], { width: 0, height: -50 });
      expect(out).toContain('viewBox="0 0 1 1"');
    });
  });

  describe('shape emission', () => {
    it('emits <rect> with resolved x/y/w/h', () => {
      const out = sceneToSvg([rect({ x: 5, y: 10, w: 100, h: 50 })], { width: 200, height: 200 });
      expect(out).toContain('<rect x="5" y="10" width="100" height="50"');
      expect(out).toContain('fill="#ff0000"');
    });

    it('resolves center rect mode to top-left', () => {
      const out = sceneToSvg(
        [rect({ x: 100, y: 100, w: 40, h: 20, mode: 'center' })],
        { width: 200, height: 200 },
      );
      expect(out).toContain('x="80" y="90" width="40" height="20"');
    });

    it('resolves corners rect mode to top-left + size', () => {
      const out = sceneToSvg(
        [rect({ x: 10, y: 20, w: 50, h: 80, mode: 'corners' })],
        { width: 200, height: 200 },
      );
      expect(out).toContain('x="10" y="20" width="40" height="60"');
    });

    it('resolves radius rect mode (x,y centre, w,h half-extents)', () => {
      const out = sceneToSvg(
        [rect({ x: 100, y: 100, w: 30, h: 40, mode: 'radius' })],
        { width: 200, height: 200 },
      );
      expect(out).toContain('x="70" y="60" width="60" height="80"');
    });

    it('emits <ellipse> with centre and radii', () => {
      const e: EllipseShape = {
        kind: 'ellipse',
        x: 100, y: 100, w: 40, h: 20,
        mode: 'corner',
        style: STYLE_RED,
      };
      const out = sceneToSvg([e], { width: 200, height: 200 });
      expect(out).toContain('<ellipse cx="120" cy="110" rx="20" ry="10"');
    });

    it('emits <line> only when a stroke is set', () => {
      const stroked: LineShape = {
        kind: 'line', x1: 0, y1: 0, x2: 100, y2: 100, style: LINE_DEFAULT_STYLE,
      };
      const out = sceneToSvg([stroked], { width: 200, height: 200 });
      expect(out).toContain('<line x1="0" y1="0" x2="100" y2="100"');
      expect(out).toContain('stroke="#000000"');
      expect(out).toContain('fill="none"');
    });

    it('skips lines with no stroke colour', () => {
      const noStroke: LineShape = {
        kind: 'line', x1: 0, y1: 0, x2: 100, y2: 100,
        style: { ...DEFAULT_STYLE, stroke: null },
      };
      const out = sceneToSvg([noStroke], { width: 200, height: 200 });
      expect(out).not.toContain('<line');
    });

    it('emits <polygon> for triangles', () => {
      const tri: TriangleShape = {
        kind: 'triangle', x1: 0, y1: 0, x2: 10, y2: 0, x3: 5, y3: 10, style: STYLE_RED,
      };
      const out = sceneToSvg([tri], { width: 100, height: 100 });
      expect(out).toContain('<polygon points="0,0 10,0 5,10"');
    });

    it('emits <polygon> when polygon is closed and <polyline> when open', () => {
      const closed: PolygonShape = {
        kind: 'polygon',
        points: [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }],
        closed: true,
        style: STYLE_RED,
      };
      const open: PolygonShape = {
        kind: 'polygon',
        points: [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }],
        closed: false,
        style: STYLE_RED_STROKE,
      };
      expect(sceneToSvg([closed], { width: 100, height: 100 })).toContain('<polygon points="0,0 10,0 10,10"');
      const openOut = sceneToSvg([open], { width: 100, height: 100 });
      expect(openOut).toContain('<polyline points="0,0 10,0 10,10"');
      expect(openOut).toContain('fill="none"');
    });

    it('emits <text> with anchor/baseline mapping', () => {
      const t: TextShape = {
        kind: 'text', text: 'hello', x: 50, y: 50,
        style: { ...DEFAULT_STYLE, textAlign: 'center', textBaseline: 'middle' },
      };
      const out = sceneToSvg([t], { width: 100, height: 100 });
      expect(out).toContain('text-anchor="middle"');
      expect(out).toContain('dominant-baseline="middle"');
      expect(out).toContain('>hello</text>');
    });

    it('escapes XML-special characters in text content', () => {
      const t: TextShape = {
        kind: 'text', text: '<bold> & "quoted"', x: 0, y: 0, style: DEFAULT_STYLE,
      };
      const out = sceneToSvg([t], { width: 100, height: 100 });
      expect(out).toContain('&lt;bold&gt; &amp; "quoted"</text>');
    });
  });

  describe('groups', () => {
    it('wraps children in <g>', () => {
      const g: GroupNode = { kind: 'group', children: [rect()] };
      const out = sceneToSvg([g], { width: 100, height: 100 });
      expect(out).toContain('<g>');
      expect(out).toContain('<rect ');
      expect(out).toContain('</g>');
    });

    it('emits transform="matrix(...)" when set, matching Canvas2D order', () => {
      const g: GroupNode = {
        kind: 'group',
        children: [rect()],
        transform: [2, 0, 0, 2, 10, 20],
      };
      const out = sceneToSvg([g], { width: 100, height: 100 });
      expect(out).toContain('transform="matrix(2 0 0 2 10 20)"');
    });

    it('emits mix-blend-mode for CSS-compatible blend modes', () => {
      const g: GroupNode = { kind: 'group', children: [rect()], blendMode: 'multiply' };
      const out = sceneToSvg([g], { width: 100, height: 100 });
      expect(out).toContain('mix-blend-mode:multiply');
    });

    it('silently drops Canvas2D compositing operators that have no SVG analog', () => {
      const g: GroupNode = { kind: 'group', children: [rect()], blendMode: 'destination-in' };
      const out = sceneToSvg([g], { width: 100, height: 100 });
      expect(out).not.toContain('mix-blend-mode');
    });

    it('wraps groups with clip rects in a <clipPath>', () => {
      const g: GroupNode = {
        kind: 'group',
        children: [rect()],
        clip: { x: 10, y: 20, w: 50, h: 60 },
      };
      const out = sceneToSvg([g], { width: 100, height: 100 });
      expect(out).toContain('<clipPath id="clip0">');
      expect(out).toContain('<rect x="10" y="20" width="50" height="60"');
      expect(out).toContain('clip-path="url(#clip0)"');
    });

    it('mints distinct clip ids for sibling clip groups', () => {
      const g1: GroupNode = {
        kind: 'group', children: [rect()], clip: { x: 0, y: 0, w: 10, h: 10 },
      };
      const g2: GroupNode = {
        kind: 'group', children: [rect()], clip: { x: 0, y: 0, w: 20, h: 20 },
      };
      const out = sceneToSvg([g1, g2], { width: 100, height: 100 });
      expect(out).toContain('id="clip0"');
      expect(out).toContain('id="clip1"');
      expect(out).toContain('url(#clip0)');
      expect(out).toContain('url(#clip1)');
    });
  });

  describe('style attributes', () => {
    it('emits fill="none" for unfilled shapes so SVG defaults do not kick in', () => {
      const shape: SceneNode = {
        kind: 'polygon',
        points: [{ x: 0, y: 0 }, { x: 10, y: 0 }],
        closed: false,
        style: STYLE_RED_STROKE,
      };
      const out = sceneToSvg([shape], { width: 100, height: 100 });
      expect(out).toContain('fill="none"');
      expect(out).toContain('stroke="#0000ff"');
      expect(out).toContain('stroke-width="3"');
    });

    it('omits default linecap/linejoin attributes', () => {
      const out = sceneToSvg([rect({ style: { ...STYLE_RED, stroke: '#000', strokeWeight: 1 } })], { width: 100, height: 100 });
      expect(out).not.toContain('stroke-linecap="butt"');
      expect(out).not.toContain('stroke-linejoin="miter"');
    });

    it('emits non-default linecap and linejoin', () => {
      const out = sceneToSvg(
        [rect({ style: { ...STYLE_RED, stroke: '#000', strokeWeight: 1, strokeCap: 'round', strokeJoin: 'bevel' } })],
        { width: 100, height: 100 },
      );
      expect(out).toContain('stroke-linecap="round"');
      expect(out).toContain('stroke-linejoin="bevel"');
    });
  });
});
