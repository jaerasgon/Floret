import { useState } from 'react';
import { Modal } from './Modal';
import { useEditor } from '../../editor/editor-state';
import { serializePatch } from '../../persistence/serialize';
import {
  createLibrary,
  listLibraries,
  upsertLibraryEntry,
} from '../../persistence/library';

/**
 * Modal for saving the current patch as an entry in a library.
 *
 * The user picks an existing library or creates a new one inline, then
 * gives the entry a display name and (optional) description. The
 * current editor state is serialised via the same path File → Save As…
 * uses, so the resulting library entry round-trips losslessly.
 *
 * The slug for a brand-new library is derived from its name (lowercased,
 * hyphenated). Re-saving to the same library name + entry name updates
 * that entry in place.
 */
interface Props {
  onClose: () => void;
  onSaved?: (librarySlug: string) => void;
}

export function SaveToLibraryModal({ onClose, onSaved }: Props) {
  const nodes = useEditor((s) => s.nodes);
  const edges = useEditor((s) => s.edges);
  // Built-in libraries are read-only — exclude them from the dropdown so
  // users can't try to save into a target that would silently fail to
  // persist.
  const [libraries, setLibraries] = useState(() =>
    listLibraries().filter((l) => !l.library.builtIn),
  );
  // null = "new library" path. Otherwise the chosen library's slug.
  const [targetSlug, setTargetSlug] = useState<string | null>(
    libraries[0]?.slug ?? null,
  );
  const [newLibName, setNewLibName] = useState('');
  const [entryName, setEntryName] = useState('');
  const [entryDesc, setEntryDesc] = useState('');
  const [error, setError] = useState<string | null>(null);

  const isNew = targetSlug === null;
  const canSave =
    entryName.trim().length > 0 &&
    (!isNew || newLibName.trim().length > 0) &&
    nodes.length > 0;

  const onSave = () => {
    setError(null);
    try {
      let slug: string;
      if (isNew) {
        const created = createLibrary(newLibName);
        slug = created.slug;
        setLibraries(listLibraries().filter((l) => !l.library.builtIn));
      } else {
        slug = targetSlug!;
      }
      // The entry id is just a stable slug from the entry name so saving
      // twice with the same name updates rather than duplicates.
      const entryId = entryName
        .trim()
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-+|-+$/g, '') || `patch-${Date.now()}`;
      const patchFile = serializePatch({ nodes, edges, name: entryName.trim() });
      upsertLibraryEntry(slug, {
        id: entryId,
        name: entryName.trim(),
        description: entryDesc.trim() || undefined,
        patch: patchFile,
      });
      onSaved?.(slug);
      onClose();
    } catch (err) {
      setError(String(err).slice(0, 200));
    }
  };

  return (
    <Modal onClose={onClose} title="Save to library" width={440}>
      <div className="modal-section">
        {libraries.length > 0 && (
          <label className="modal-field">
            <span className="modal-field-label">Library</span>
            <select
              className="modal-select"
              value={isNew ? '__new__' : targetSlug ?? ''}
              onChange={(e) => setTargetSlug(e.target.value === '__new__' ? null : e.target.value)}
            >
              {libraries.map((l) => (
                <option key={l.slug} value={l.slug}>
                  {l.library.name} ({l.library.patches.length})
                </option>
              ))}
              <option value="__new__">＋ New library…</option>
            </select>
          </label>
        )}
        {(isNew || libraries.length === 0) && (
          <label className="modal-field">
            <span className="modal-field-label">New library name</span>
            <input
              className="modal-input"
              placeholder="New library name"
              value={newLibName}
              onChange={(e) => setNewLibName(e.target.value)}
            />
          </label>
        )}
      </div>

      <div className="modal-section">
        <label className="modal-field">
          <span className="modal-field-label">Patch name</span>
          <input
            className="modal-input"
            placeholder="Patch name (shown in the menu)"
            value={entryName}
            onChange={(e) => setEntryName(e.target.value)}
          />
        </label>
        <label className="modal-field">
          <span className="modal-field-label">Description <span className="modal-field-optional">(optional)</span></span>
          <textarea
            className="modal-textarea"
            placeholder="Description (optional)"
            value={entryDesc}
            onChange={(e) => setEntryDesc(e.target.value)}
            rows={3}
          />
        </label>
      </div>

      {error && <div className="library-error">{error}</div>}
      {nodes.length === 0 && (
        <div className="library-empty">Current patch is empty — nothing to save.</div>
      )}

      <div className="modal-actions">
        <button type="button" className="modal-btn modal-btn-ghost" onClick={onClose}>Cancel</button>
        <button type="button" className="modal-btn modal-btn-primary" disabled={!canSave} onClick={onSave}>Save</button>
      </div>
    </Modal>
  );
}
