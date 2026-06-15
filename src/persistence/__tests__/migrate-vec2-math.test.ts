import { describe, expect, it } from 'vitest';
import { migratePatch } from '../migrate';
import { CURRENT_PATCH_VERSION, type PatchFile } from '../schema';

function v1Patch(): PatchFile {
  return {
    version: 1,
    nodes: [
      { id: 'a', defType: 'value/number', params: {} },
      { id: 'add', defType: 'vec2/add', params: {} },
      { id: 'lerp', defType: 'vec2/lerp', params: {} },
      { id: 'scale', defType: 'vec2/scale', params: {} }, // stays (mixed-type)
      { id: 'rect', defType: 'draw/rect', params: {} },
    ],
    edges: [
      { id: 'e1', source: 'add', target: 'rect', sourceHandle: 'out-vector', targetHandle: 'in-pos' },
      { id: 'e2', source: 'lerp', target: 'add', sourceHandle: 'out-vector', targetHandle: 'in-a' },
      // an unrelated edge using vec2/scale's own output handle — untouched
      { id: 'e3', source: 'scale', target: 'rect', sourceHandle: 'out-scaled', targetHandle: 'in-pos' },
    ],
  } as unknown as PatchFile;
}

describe('v1→v2 migration: vec2 arithmetic → adaptive math', () => {
  it('rewrites vec2/add + vec2/lerp to math equivalents with valueType=vec2', () => {
    const { patch } = migratePatch(v1Patch());
    expect(patch.version).toBe(CURRENT_PATCH_VERSION);
    const find = (id: string) => patch.nodes.find((n) => n.id === id)!;
    expect(find('add').defType).toBe('math/add');
    expect(find('add').params.valueType).toBe('vec2');
    expect(find('lerp').defType).toBe('math/lerp');
    expect(find('lerp').params.valueType).toBe('vec2');
  });

  it('leaves vec2/scale (mixed-type) untouched', () => {
    const { patch } = migratePatch(v1Patch());
    const scale = patch.nodes.find((n) => n.id === 'scale')!;
    expect(scale.defType).toBe('vec2/scale');
    expect(scale.params.valueType).toBeUndefined();
  });

  it('remaps the migrated nodes\' vector output handle to result', () => {
    const { patch } = migratePatch(v1Patch());
    const e1 = patch.edges.find((e) => e.id === 'e1')!;
    const e2 = patch.edges.find((e) => e.id === 'e2')!;
    expect(e1.sourceHandle).toBe('out-result'); // add → rect
    expect(e2.sourceHandle).toBe('out-result'); // lerp → add
    // operand handle untouched
    expect(e2.targetHandle).toBe('in-a');
    // vec2/scale output handle left alone
    const e3 = patch.edges.find((e) => e.id === 'e3')!;
    expect(e3.sourceHandle).toBe('out-scaled');
  });
});
