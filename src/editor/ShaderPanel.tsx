import { useEffect, useMemo, useState } from 'react';
import { useEditor } from './editor-state';
import { Panel } from './panel/Panel';
import { parseUniforms, uniformPortType } from '../runtime/gl/glsl-shader';
import { checkShaderCompile, type ShaderError } from '../runtime/gl/shader-compile-check';

/**
 * Shader panel — a GLSL editor for the currently-selected `gpu/shader` node.
 *
 * A full dock panel (draggable / dockable / resizable like Help / Preview).
 * Mounts on selection and floats by default; App keeps `'shader'` in the dock
 * reconcile keep-set while a shader node is selected so it isn't evicted. Edits
 * the node's `source` param (debounced), compile-checks on the main thread for
 * line-mapped errors, lists the auto-detected uniforms, and toggles self-feedback.
 */
export function ShaderPanel() {
  const selected = useEditor((s) => s.nodes.find((n) => n.selected && n.data.defType === 'gpu/shader'));
  if (!selected) return null;
  return (
    <Panel id="shader" title="Shader · GLSL" defaults={{ dock: 'float', width: 420, height: 560, floatX: 360, floatY: 90 }}>
      <ShaderEditor
        key={selected.id}
        nodeId={selected.id}
        initialSource={String(selected.data.params?.source ?? '')}
        initialFeedback={Boolean(selected.data.params?.feedback)}
      />
    </Panel>
  );
}

function ShaderEditor({ nodeId, initialSource, initialFeedback }: {
  nodeId: string; initialSource: string; initialFeedback: boolean;
}) {
  const setNodeParam = useEditor((s) => s.setNodeParam);
  const [text, setText] = useState(initialSource);
  const [errors, setErrors] = useState<ShaderError[]>([]);
  const [feedback, setFeedback] = useState(initialFeedback);

  // Debounce: commit the source + recompile-check 300ms after the last keystroke.
  useEffect(() => {
    const h = setTimeout(() => {
      setNodeParam(nodeId, 'source', text);
      setErrors(checkShaderCompile(text).errors);
    }, 300);
    return () => clearTimeout(h);
  }, [text, nodeId, setNodeParam]);

  // Initial compile-check on mount / node switch.
  useEffect(() => { setErrors(checkShaderCompile(initialSource).errors); }, [initialSource]);

  const uniforms = useMemo(() => parseUniforms(text), [text]);

  return (
    <div className="shader-panel">
      <textarea
        className="shader-panel-code nodrag"
        spellCheck={false}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          // Tab inserts two spaces instead of leaving the field.
          if (e.key === 'Tab') {
            e.preventDefault();
            const t = e.currentTarget;
            const s = t.selectionStart, en = t.selectionEnd;
            const next = text.slice(0, s) + '  ' + text.slice(en);
            setText(next);
            requestAnimationFrame(() => { t.selectionStart = t.selectionEnd = s + 2; });
          }
        }}
      />

      {errors.length > 0 && (
        <div className="shader-panel-errors">
          {errors.map((e, i) => (
            <div key={i} className="shader-panel-error">
              {e.line > 0 ? <span className="shader-panel-error-line">L{e.line}</span> : null} {e.message}
            </div>
          ))}
        </div>
      )}

      <div className="shader-panel-meta">
        <label className="shader-panel-toggle">
          <input
            type="checkbox"
            checked={feedback}
            onChange={(e) => { setFeedback(e.target.checked); setNodeParam(nodeId, 'feedback', e.target.checked); }}
          />
          Feedback <span className="shader-panel-dim">(previous frame → iChannel1)</span>
        </label>

        <div className="shader-panel-uniforms">
          <div className="shader-panel-uniforms-title">Uniforms → ports</div>
          {uniforms.length === 0
            ? <div className="shader-panel-dim">none — declare e.g. <code>uniform float k;</code></div>
            : uniforms.map((u) => (
                <div key={u.name} className="shader-panel-uniform">
                  <span className="shader-panel-uniform-name">{u.name}</span>
                  <span className="shader-panel-dim">{u.glslType} → {uniformPortType(u.glslType)}</span>
                </div>
              ))}
        </div>
        <div className="shader-panel-dim shader-panel-builtins">
          built-in: iTime · iResolution · iMouse · iFrame · iChannel0 (input)
        </div>
      </div>
    </div>
  );
}
