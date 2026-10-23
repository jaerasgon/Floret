import { registerNode } from '../registry';
import { CommentNode } from '../../editor/nodes/CommentNode';

/**
 * Comment — a free-floating note for annotating patches. No pins, no
 * compute. Editor-only. Runtime sees it as a no-op (no compute,
 * empty outputs) and ignores it.
 */
export const commentNode = registerNode({
  type: 'meta/comment',
  category: 'Meta',
  label: 'Comment',
  description: 'Annotate the patch with a free-floating note.',
  inputs: [],
  outputs: [],
  defaultParams: { text: 'comment', width: 200, height: 80 },
  component: CommentNode,
});
