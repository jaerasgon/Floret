import { memo, useCallback, useEffect, useRef, useState } from 'react';
import { NodeResizer, type NodeProps } from '@xyflow/react';
import { useEditor, type FloretNodeData } from '../editor-state';

/**
 * Free-floating comment box. No pins, no compute. Just a draggable,
 * resizable text card for annotating patches.
 *
 *   - Double-click → enter edit mode (textarea)
 *   - Blur or Esc → commit
 *   - Drag corner → resize (NodeResizer)
 *   - Drag body  → move (React Flow default)
 */
export const CommentNode = memo(function CommentNode(props: NodeProps) {
  const { id, data, selected } = props;
  const nodeData = data as unknown as FloretNodeData;
  const setNodeParam = useEditor((s) => s.setNodeParam);

  const w = (nodeData.params.width as number | undefined) ?? 200;
  const h = (nodeData.params.height as number | undefined) ?? 80;
  const text = (nodeData.params.text as string | undefined) ?? '';

  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(text);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  // Sync drafts when text changes from elsewhere (undo, paste, etc.).
  useEffect(() => {
    if (!editing) setDraft(text);
  }, [text, editing]);

  useEffect(() => {
    if (editing) {
      textareaRef.current?.focus();
      textareaRef.current?.select();
    }
  }, [editing]);

  const commit = useCallback(() => {
    setEditing(false);
    if (draft !== text) setNodeParam(id, 'text', draft);
  }, [draft, text, id, setNodeParam]);

  const onResizeEnd = useCallback(
    (_e: unknown, p: { width: number; height: number }) => {
      setNodeParam(id, 'width', p.width);
      setNodeParam(id, 'height', p.height);
    },
    [id, setNodeParam],
  );

  return (
    <div
      className="comment-node"
      data-selected={selected ? 1 : 0}
      data-editing={editing ? 1 : 0}
      style={{ width: w, height: h }}
      onDoubleClick={(e) => {
        e.stopPropagation();
        setDraft(text);
        setEditing(true);
      }}
    >
      <NodeResizer
        minWidth={120}
        minHeight={48}
        isVisible={selected}
        onResizeEnd={onResizeEnd}
        lineClassName="comment-resize-line"
        handleClassName="comment-resize-handle"
      />
      {editing ? (
        <textarea
          ref={textareaRef}
          className="comment-textarea nodrag"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              setDraft(text);
              setEditing(false);
            }
            // Ctrl/Cmd+Enter or just Enter — commit
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
              commit();
            }
          }}
        />
      ) : (
        <div className="comment-body">{text || <span className="comment-placeholder">double-click to edit</span>}</div>
      )}
    </div>
  );
});
