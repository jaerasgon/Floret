import { describe, expect, it } from 'vitest';
import { encodePatchPayload, decodePatchPayload } from '../share';
import { serializePatch } from '../serialize';
import { SAMPLES } from '../samples';
import type { FloretNode } from '../../editor/editor-state';

describe('share link round-trip', () => {
  it('encodes and decodes a small patch losslessly', async () => {
    const patch = serializePatch({
      nodes: [
        { id: 'a', type: 'floret', position: { x: 0, y: 0 }, data: { defType: 'value/number', params: { value: 42 } } },
      ] as unknown as FloretNode[],
      edges: [],
      name: 'tiny',
    });
    const payload = await encodePatchPayload(patch);
    const decoded = await decodePatchPayload(payload);
    expect(decoded.nodes).toHaveLength(1);
    expect(decoded.nodes[0]!.params).toMatchObject({ value: 42 });
    expect(decoded.meta?.name).toBe('tiny');
  });

  it('uses the gzip codec marker when CompressionStream is available', async () => {
    const patch = serializePatch({ nodes: [], edges: [], name: 'x' });
    const payload = await encodePatchPayload(patch);
    // 'g' = gzip, 'r' = raw fallback. The Node test env has CompressionStream.
    expect(['g', 'r']).toContain(payload[0]);
    if (typeof CompressionStream !== 'undefined') expect(payload[0]).toBe('g');
  });

  it('round-trips a real, sizeable sample patch', async () => {
    // Grid Art is one of the larger samples — exercises chunked base64.
    const sample = SAMPLES.find((s) => s.id === 'grid-art')!;
    const { nodes, edges } = sample.build();
    const patch = serializePatch({ nodes, edges, name: sample.name });
    const payload = await encodePatchPayload(patch);
    const decoded = await decodePatchPayload(payload);
    expect(decoded.nodes).toHaveLength(patch.nodes.length);
    expect(decoded.edges).toHaveLength(patch.edges.length);
  });

  it('produces a URL-safe payload (no +, /, or = chars)', async () => {
    const sample = SAMPLES.find((s) => s.id === 'grid-art')!;
    const { nodes, edges } = sample.build();
    const payload = await encodePatchPayload(serializePatch({ nodes, edges, name: 'g' }));
    expect(payload.slice(1)).not.toMatch(/[+/=]/);
  });

  it('rejects a payload that does not decode to a patch', async () => {
    // 'r' marker + base64url of "not a patch"
    const bad = 'r' + Buffer.from('"nope"').toString('base64').replace(/=+$/, '');
    await expect(decodePatchPayload(bad)).rejects.toThrow();
  });
});
