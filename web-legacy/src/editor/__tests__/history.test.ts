import { beforeEach, describe, expect, it } from 'vitest';
import { useEditor } from '../editor-state';
import { paramStore } from '../../runtime/param-store';

// Side-effect imports — register builtins (addNode needs them) AND wire
// history → editor-state via the history module's side effect.
import '../../nodes';
import { history } from '../history';

beforeEach(() => {
  useEditor.getState().loadPatch([], []);
  paramStore.replaceAll([]);
  history.reset();
});

describe('undo / redo', () => {
  it('undo reverts an addNode, redo replays it', () => {
    const id = useEditor.getState().addNode('value/number', { x: 0, y: 0 });
    expect(useEditor.getState().nodes).toHaveLength(1);

    history.undo();
    expect(useEditor.getState().nodes).toHaveLength(0);

    history.redo();
    expect(useEditor.getState().nodes).toHaveLength(1);
    expect(useEditor.getState().nodes[0]!.id).toBe(id);
  });

  it('setNodeParam pushes an entry; coalesces same-key rapid edits', () => {
    const id = useEditor.getState().addNode('value/number', { x: 0, y: 0 });
    useEditor.getState().setNodeParam(id, 'value', 1);
    useEditor.getState().setNodeParam(id, 'value', 2);
    useEditor.getState().setNodeParam(id, 'value', 3);

    // After all rapid edits, undo should jump back to the value BEFORE
    // any of the commits — i.e. to the seed value from addNode (0).
    history.undo();
    const params = useEditor.getState().nodes[0]!.data.params;
    expect(params.value).toBe(0);
  });

  it('redo stack clears when a new mutation lands', () => {
    useEditor.getState().addNode('value/number', { x: 0, y: 0 });
    useEditor.getState().addNode('value/number', { x: 100, y: 0 });
    history.undo();
    expect(history.canRedo()).toBe(true);

    useEditor.getState().addNode('math/sin', { x: 200, y: 0 });
    expect(history.canRedo()).toBe(false);
  });

  it('undo of a remove restores nodes AND their ParamStore entries', () => {
    const id = useEditor.getState().addNode('io/slider', { x: 0, y: 0 });
    useEditor.getState().setNodeParam(id, 'value', 0.42);

    // Select & remove the node.
    useEditor.setState((s) => ({
      nodes: s.nodes.map((n) => (n.id === id ? { ...n, selected: true } : n)),
    }));
    useEditor.getState().removeSelected();
    expect(useEditor.getState().nodes).toHaveLength(0);
    expect(paramStore.getNode(id)).toBeUndefined();

    history.undo();
    expect(useEditor.getState().nodes).toHaveLength(1);
    expect(paramStore.getKey(id, 'value')).toBe(0.42);
  });
});
