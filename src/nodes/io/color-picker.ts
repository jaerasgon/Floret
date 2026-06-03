import { registerNode } from '../registry';

function hexToRgb(hex: string): [number, number, number] {
  const m = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  if (!m) return [255, 255, 255];
  return [parseInt(m[1]!, 16), parseInt(m[2]!, 16), parseInt(m[3]!, 16)];
}

/**
 * Color picker — a single hex param backs the inline colour swatch in
 * GenericNode (the editor recognises `paramKey === 'hex'` and renders
 * `<input type="color">`). Outputs a Color value plus the r/g/b
 * channels in 0..255.
 */
export const colorPickerNode = registerNode({
  type: 'io/color-picker',
  category: 'IO',
  label: 'ColorPicker',
  description: 'Pick a colour and its r/g/b channels.',
  inputs: [],
  outputs: [
    { name: 'color', type: 'color' },
    { name: 'r', type: 'number' },
    { name: 'g', type: 'number' },
    { name: 'b', type: 'number' },
  ],
  defaultParams: { hex: '#ffffff' },
  readFromContext: (_frame, params) => {
    const hex = params.param<string>('hex', '#ffffff');
    const [r, g, b] = hexToRgb(hex);
    return [[{ r: r / 255, g: g / 255, b: b / 255, a: 1 }], [r], [g], [b]];
  },
});
