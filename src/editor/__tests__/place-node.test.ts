import { describe, it, expect, beforeEach } from 'vitest';
import '../../nodes';
import { nodeRegistry } from '../../nodes/registry';
import { useEditor } from '../editor-state';
import { useOverlay } from '../overlay-store';
import { placeNodeAtFlow } from '../place-node';

beforeEach(() => {
  useEditor.getState().loadPatch([], []);
  useOverlay.getState().disarmRegionDraw();
});

describe('placeNodeAtFlow', () => {
  it('auto-parents a node dropped inside a region', () => {
    const regionId = useEditor
      .getState()
      .addRegion('foreach', { x: 0, y: 0 }, { width: 400, height: 300 });
    const def = nodeRegistry.require('value/number');

    const id = placeNodeAtFlow(def, { x: 100, y: 100 });

    expect(id).not.toBeNull();
    const node = useEditor.getState().nodes.find((n) => n.id === id);
    expect(node?.parentId).toBe(regionId);
    // Local position is offset from the region origin (0,0).
    expect(node?.position).toEqual({ x: 100, y: 100 });
  });

  it('leaves a node dropped outside any region at root scope', () => {
    useEditor.getState().addRegion('foreach', { x: 0, y: 0 }, { width: 200, height: 200 });
    const def = nodeRegistry.require('value/number');

    const id = placeNodeAtFlow(def, { x: 999, y: 999 });

    const node = useEditor.getState().nodes.find((n) => n.id === id);
    expect(node?.parentId ?? null).toBeNull();
  });

  it('arms the region draw tool for a drawable region def and creates no node', () => {
    const def = nodeRegistry.require('region/foreach');

    const result = placeNodeAtFlow(def, { x: 0, y: 0 });

    expect(result).toBeNull();
    expect(useOverlay.getState().armedRegionKind).toBe('foreach');
    expect(useEditor.getState().nodes).toHaveLength(0);
  });
});
