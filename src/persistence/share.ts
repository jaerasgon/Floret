import type { PatchFile } from './schema';
import { isPatchFile } from './schema';
import { migratePatch } from './migrate';

/**
 * Shareable patch links. A whole patch is compressed (gzip via the
 * platform CompressionStream) and packed into the URL hash, so a sketch
 * can be shared with nothing but a link — no backend, no storage.
 *
 *   https://app/#patch=g<base64url-of-gzipped-json>
 *
 * The first character after `#patch=` is a codec marker: `g` = gzip, `r`
 * = raw (uncompressed fallback for the rare environment without
 * CompressionStream). Decoders read the marker, so a link always round-
 * trips within any single browser and across browsers that share gzip
 * support (all current targets).
 */

const HASH_KEY = 'patch';
const HASH_RE = new RegExp(`[#&]${HASH_KEY}=([^&]+)`);

// ── base64url ↔ bytes ──────────────────────────────────────────────────────
function bytesToBase64Url(bytes: Uint8Array): string {
  let bin = '';
  const CHUNK = 0x8000; // avoid arg-count overflow on String.fromCharCode
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function base64UrlToBytes(s: string): Uint8Array {
  const b64 = s.replace(/-/g, '+').replace(/_/g, '/') + '==='.slice((s.length + 3) % 4);
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

// ── gzip via the platform streams (zero-dep) ───────────────────────────────
async function gzip(bytes: Uint8Array): Promise<Uint8Array | null> {
  if (typeof CompressionStream === 'undefined') return null;
  const stream = new Blob([bytes as BlobPart]).stream().pipeThrough(new CompressionStream('gzip'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function gunzip(bytes: Uint8Array): Promise<Uint8Array> {
  if (typeof DecompressionStream === 'undefined') {
    throw new Error('DecompressionStream unavailable — cannot read a gzipped link here.');
  }
  const stream = new Blob([bytes as BlobPart]).stream().pipeThrough(new DecompressionStream('gzip'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** Encode a patch to the `g…`/`r…` payload string (no `#patch=` prefix). */
export async function encodePatchPayload(patch: PatchFile): Promise<string> {
  const json = JSON.stringify(patch);
  const bytes = new TextEncoder().encode(json);
  const zipped = await gzip(bytes);
  if (zipped) return 'g' + bytesToBase64Url(zipped);
  return 'r' + bytesToBase64Url(bytes);
}

/** Decode a payload string back to a PatchFile (migrated to current schema). */
export async function decodePatchPayload(payload: string): Promise<PatchFile> {
  const marker = payload[0];
  const body = base64UrlToBytes(payload.slice(1));
  const bytes = marker === 'g' ? await gunzip(body) : body;
  const json = new TextDecoder().decode(bytes);
  const raw = JSON.parse(json);
  if (!isPatchFile(raw)) throw new Error('Shared link does not contain a valid patch.');
  return migratePatch(raw).patch;
}

/** Build a full shareable URL for the current location + a patch. */
export async function buildShareUrl(patch: PatchFile): Promise<string> {
  const payload = await encodePatchPayload(patch);
  const base = `${window.location.origin}${window.location.pathname}`;
  return `${base}#${HASH_KEY}=${payload}`;
}

/**
 * If the current URL carries a shared patch, decode and return it, then
 * strip the hash so a reload doesn't re-trigger the import. Returns null
 * when there's no shared patch (or it failed to decode — caller decides
 * how loudly to surface that).
 */
export async function consumeSharedPatch(): Promise<PatchFile | null> {
  if (typeof window === 'undefined') return null;
  const m = HASH_RE.exec(window.location.hash);
  if (!m || !m[1]) return null;
  const payload = m[1];
  // Strip the hash up front so a decode failure doesn't strand a poison
  // link that re-throws on every reload.
  const url = `${window.location.origin}${window.location.pathname}${window.location.search}`;
  window.history.replaceState(null, '', url);
  return decodePatchPayload(payload);
}

export function hasSharedPatch(): boolean {
  return typeof window !== 'undefined' && HASH_RE.test(window.location.hash);
}
