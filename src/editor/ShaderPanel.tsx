import { useEffect, useMemo, useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { useEditor } from './editor-state';
import { Panel } from './panel/Panel';
import { parseUniforms, uniformPortType } from '../runtime/gl/glsl-shader';
import { checkShaderCompile, type ShaderError } from '../runtime/gl/shader-compile-check';
import { GlslEditor } from './GlslEditor';
import type { ShaderBufferId } from '../runtime/scene';

/**
 * Shader panel — a GLSL editor for the selected `gpu/shader` node.
 *
 * A full dock panel (draggable / dockable / resizable). Tabs switch between the
 * Image pass and Buffer A–D passes; each pass has its own source + four iChannel
 * routings (input / self / another buffer). Edits debounce into the node's
 * params (which re-derive the uniform ports + recompile), with main-thread
 * line-mapped compile errors and the detected uniform list.
 */
export function ShaderPanel() {
  const selectedId = useEditor((s) => s.nodes.find((n) => n.selected && n.data.defType === 'gpu/shader')?.id ?? null);
  if (!selectedId) return null;
  return (
    <Panel id="shader" title="Shader · GLSL" defaults={{ dock: 'float', width: 440, height: 600, floatX: 360, floatY: 80 }}>
      <ShaderEditor key={selectedId} nodeId={selectedId} />
    </Panel>
  );
}

const ALL_BUFS: ShaderBufferId[] = ['A', 'B', 'C', 'D'];
const DEFAULT_BUFFER = `// Buffer pass. iChannel0 is this buffer's previous frame (self).
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
  vec2 uv = fragCoord / iResolution.xy;
  fragColor = texture(iChannel0, uv);
}`;
const paramKey = (tab: ShaderBufferId) => (tab === 'image' ? 'source' : 'buffer' + tab);

function ShaderEditor({ nodeId }: { nodeId: string }) {
  const params = useEditor((s) => s.nodes.find((n) => n.id === nodeId)?.data.params ?? {}) as Record<string, unknown>;
  const setNodeParam = useEditor((s) => s.setNodeParam);

  const present = ALL_BUFS.filter((b) => typeof params['buffer' + b] === 'string' && (params['buffer' + b] as string).trim());
  const tabs: ShaderBufferId[] = ['image', ...present];
  const [activeTab, setActiveTab] = useState<ShaderBufferId>('image');
  const tab = tabs.includes(activeTab) ? activeTab : 'image';

  const srcKey = paramKey(tab);
  const committed = String(params[srcKey] ?? '');
  const [text, setText] = useState(committed);
  const [errors, setErrors] = useState<ShaderError[]>([]);

  // Load the active pass's source when the tab (or node) changes.
  useEffect(() => { setText(String(params[paramKey(tab)] ?? '')); setErrors([]); /* eslint-disable-next-line */ }, [tab, nodeId]);

  // Debounced commit + compile-check 300ms after the last keystroke.
  useEffect(() => {
    const h = setTimeout(() => {
      setNodeParam(nodeId, srcKey, text);
      setErrors(checkShaderCompile(text).errors);
    }, 300);
    return () => clearTimeout(h);
  }, [text, srcKey, nodeId, setNodeParam]);

  const uniforms = useMemo(() => parseUniforms(text).filter((u) => u.glslType !== 'sampler2D'), [text]);

  // Channel routing for the active pass.
  const channelCfg = (params.channels && typeof params.channels === 'object' ? params.channels : {}) as Record<string, unknown>;
  const tabChannels = Array.isArray(channelCfg[tab])
    ? (channelCfg[tab] as unknown[])
    : tab === 'image' ? ['input', params.feedback ? 'image' : '', '', ''] : [tab, 'input', '', ''];
  const channelOptions: { value: string; label: string }[] = [
    { value: '', label: 'none' },
    { value: 'input', label: 'input' },
    ...tabs.filter((t) => t !== tab).map((t) => ({ value: t, label: t === 'image' ? 'Image' : 'Buffer ' + t })),
    { value: tab, label: 'self' },
  ];
  const setChannel = (i: number, code: string) => {
    const next = { ...channelCfg };
    const arr = (Array.isArray(next[tab]) ? [...(next[tab] as unknown[])] : [...tabChannels]);
    arr[i] = code;
    next[tab] = arr;
    setNodeParam(nodeId, 'channels', next);
  };

  const addBuffer = () => {
    const free = ALL_BUFS.find((b) => !present.includes(b));
    if (!free) return;
    setNodeParam(nodeId, 'buffer' + free, DEFAULT_BUFFER);
    setActiveTab(free);
  };
  const removeBuffer = (b: ShaderBufferId) => {
    setNodeParam(nodeId, 'buffer' + b, '');
    const next = { ...channelCfg }; delete next[b]; setNodeParam(nodeId, 'channels', next);
    setActiveTab('image');
  };

  return (
    <div className="shader-panel">
      <div className="shader-tabs">
        {tabs.map((t) => (
          <button
            key={t}
            className={`shader-tab${t === tab ? ' active' : ''}`}
            onClick={() => setActiveTab(t)}
          >{t === 'image' ? 'Image' : 'Buf ' + t}</button>
        ))}
        {present.length < 4 && (
          <button className="shader-tab shader-tab-add" title="Add a buffer pass" onClick={addBuffer}>
            <Plus size={12} strokeWidth={2} />
          </button>
        )}
        {tab !== 'image' && (
          <button className="shader-tab shader-tab-del" title={`Remove Buffer ${tab}`} onClick={() => removeBuffer(tab)}>
            <Trash2 size={12} strokeWidth={1.8} />
          </button>
        )}
      </div>

      <GlslEditor value={text} onChange={setText} />

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
        <div className="shader-channels">
          <div className="shader-channels-title">iChannels (this pass)</div>
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className="shader-channel-row">
              <span className="shader-channel-label">iChannel{i}</span>
              <select
                className="shader-channel-select nodrag"
                value={String(tabChannels[i] ?? '')}
                onChange={(e) => setChannel(i, e.target.value)}
              >
                {channelOptions.map((o) => <option key={o.value || 'none'} value={o.value}>{o.label}</option>)}
              </select>
            </div>
          ))}
        </div>

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
          built-in: iTime · iResolution · iMouse · iFrame
        </div>
      </div>
    </div>
  );
}
