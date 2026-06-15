import { CURRENT_PATCH_VERSION, type PatchFile } from './schema';

/**
 * Migration ladder: each entry takes a patch at version N and returns one
 * at version N+1. The runner walks from `patch.version` up to current.
 *
 * To add a v1→v2 migration, push a new entry and bump CURRENT_PATCH_VERSION.
 */

type Migration = (patch: PatchFile) => PatchFile;

// v1 → v2: the vec2 arithmetic nodes were folded into the adaptive math
// nodes (one polymorphic node per op). Rewrite the vec2 ops to their math
// equivalent with `valueType: 'vec2'`, and remap their `vector` output handle
// to the adaptive `result` handle. Operand handles (a/b/t) already match.
const VEC2_TO_MATH: Record<string, string> = {
  'vec2/add': 'math/add',
  'vec2/sub': 'math/sub',
  'vec2/mul': 'math/mul',
  'vec2/lerp': 'math/lerp',
};

const migrations: Migration[] = [
  // v1 → v2
  (patch) => {
    const migratedIds = new Set<string>();
    const nodes = patch.nodes.map((n) => {
      const mapped = VEC2_TO_MATH[n.defType];
      if (!mapped) return n;
      migratedIds.add(n.id);
      return { ...n, defType: mapped, params: { ...n.params, valueType: 'vec2' } };
    });
    const edges = patch.edges.map((e) =>
      migratedIds.has(e.source) && e.sourceHandle === 'out-vector'
        ? { ...e, sourceHandle: 'out-result' }
        : e,
    );
    return { ...patch, version: 2, nodes, edges };
  },
];

export interface MigrateResult {
  patch: PatchFile;
  applied: string[];
}

export function migratePatch(patch: PatchFile): MigrateResult {
  const applied: string[] = [];
  let cur = patch;

  if (cur.version > CURRENT_PATCH_VERSION) {
    throw new Error(
      `patch version ${cur.version} is newer than this app (max ${CURRENT_PATCH_VERSION}). ` +
        'Please update Floret.',
    );
  }

  while (cur.version < CURRENT_PATCH_VERSION) {
    const step = migrations[cur.version - 1];
    if (!step) {
      throw new Error(
        `no migration from v${cur.version} → v${cur.version + 1}. This is a bug.`,
      );
    }
    cur = step(cur);
    applied.push(`v${cur.version - 1}→v${cur.version}`);
  }

  return { patch: cur, applied };
}
