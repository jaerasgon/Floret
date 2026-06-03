import { CURRENT_PATCH_VERSION, type PatchFile } from './schema';

/**
 * Migration ladder: each entry takes a patch at version N and returns one
 * at version N+1. The runner walks from `patch.version` up to current.
 *
 * To add a v1→v2 migration, push a new entry and bump CURRENT_PATCH_VERSION.
 */

type Migration = (patch: PatchFile) => PatchFile;

const migrations: Migration[] = [
  // v1 → v2 (placeholder; uncomment + adapt when needed)
  // (patch) => ({ ...patch, version: 2, /* renames, defaults, etc. */ }),
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
        'Please update Nodule.',
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
