import { useCallback, useSyncExternalStore } from 'react';
import { paramStore } from '../runtime/param-store';
import { useEditor } from './editor-state';

/**
 * Subscribe an IO-box component to its own param in the ParamStore.
 *
 * Returns `[value, setLive, commit]`:
 *   - `value`     — current value, kept in sync via useSyncExternalStore.
 *                   ZERO React Flow reconcile on update.
 *   - `setLive(v)`— write to the ParamStore. The runtime sees the new
 *                   value on the next tick. Call this on every pointer
 *                   event during a drag (no React reconcile).
 *   - `commit()`  — copy the current ParamStore value into editor state
 *                   (`node.data.params`). Call on pointerup, blur, or
 *                   change-event for non-drag controls. This is what
 *                   makes save/load and (future) undo see the value.
 */
export function useNodeParam<T>(
  nodeId: string,
  key: string,
  fallback: T,
): [T, (v: T) => void, () => void] {
  const value = useSyncExternalStore<T>(
    (cb) => paramStore.subscribe(nodeId, cb),
    () => {
      const v = paramStore.getKey<T>(nodeId, key);
      return v !== undefined ? v : fallback;
    },
    () => fallback,
  );

  const setLive = useCallback(
    (v: T) => {
      paramStore.setKey(nodeId, key, v);
    },
    [nodeId, key],
  );

  const commit = useCallback(() => {
    const v = paramStore.getKey(nodeId, key);
    if (v !== undefined) {
      useEditor.getState().setNodeParam(nodeId, key, v);
    }
  }, [nodeId, key]);

  return [value, setLive, commit];
}
