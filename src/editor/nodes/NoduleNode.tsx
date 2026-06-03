import { memo } from 'react';
import { type NodeProps } from '@xyflow/react';
import { nodeRegistry } from '../../nodes/registry';
import { GenericNode } from './GenericNode';
import type { NoduleNodeData } from '../editor-state';

/**
 * Single dispatcher type registered with React Flow as 'nodule'.
 * Each NodeDef can override the visual via `def.component`; otherwise
 * GenericNode renders typed handles + an inline param editor.
 */
export const NoduleNode = memo(function NoduleNode(props: NodeProps) {
  const nodeData = props.data as unknown as NoduleNodeData;
  const def = nodeRegistry.get(nodeData.defType);
  if (def?.component) {
    const Comp = def.component;
    return <Comp {...props} />;
  }
  return <GenericNode {...props} />;
});
