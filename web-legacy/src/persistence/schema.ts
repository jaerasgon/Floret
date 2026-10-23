/**
 * Versioned patch schema. Save files are JSON conforming to `PatchFile`.
 *
 * Versioning policy:
 *   - Each save embeds a `version` number.
 *   - Loaders run migrations (see migrate.ts) to bring older versions up to
 *     the current `CURRENT_PATCH_VERSION`.
 *   - Never repurpose a field; add a new one and migrate.
 */

export const CURRENT_PATCH_VERSION = 2;

export interface SerializedNode {
  id: string;
  defType: string;
  position: { x: number; y: number };
  params: Record<string, unknown>;
  /** Parent region id (omitted for root-scope nodes). */
  parentId?: string;
  /** Per-input-port constant overrides — see FloretNodeData.portValues. */
  portValues?: Record<string, unknown>;
  /** Dynamic frame ports on region nodes. */
  ports?: SerializedRegionPort[];
  /** Node visual size overrides (regions, IO boxes). */
  width?: number;
  height?: number;
}

export interface SerializedRegionPort {
  id: string;
  name: string;
  type: string;
  direction: 'input' | 'output';
}

export interface SerializedEdge {
  id: string;
  source: string;
  target: string;
  sourceHandle: string | null;
  targetHandle: string | null;
}

export interface PatchFile {
  /** Format marker. */
  kind: 'floret.patch';
  version: number;
  /** App version that wrote the file (for diagnostics — not used for migration). */
  appVersion?: string;
  /** Optional human metadata. */
  meta?: {
    name?: string;
    author?: string;
    createdAt?: string; // ISO 8601
    modifiedAt?: string;
  };
  nodes: SerializedNode[];
  edges: SerializedEdge[];
}

export function emptyPatch(name?: string): PatchFile {
  const now = new Date().toISOString();
  return {
    kind: 'floret.patch',
    version: CURRENT_PATCH_VERSION,
    appVersion: '0.0.1',
    meta: { name, createdAt: now, modifiedAt: now },
    nodes: [],
    edges: [],
  };
}

export function isPatchFile(v: unknown): v is PatchFile {
  if (!v || typeof v !== 'object') return false;
  const obj = v as Record<string, unknown>;
  return (
    obj.kind === 'floret.patch' &&
    typeof obj.version === 'number' &&
    Array.isArray(obj.nodes) &&
    Array.isArray(obj.edges)
  );
}
