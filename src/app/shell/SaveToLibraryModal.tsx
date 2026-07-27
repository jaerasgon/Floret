import { useState } from 'react';
import { Dialog, DialogHeader } from '@astryxdesign/core/Dialog';
import { Selector } from '@astryxdesign/core/Selector';
import { TextInput } from '@astryxdesign/core/TextInput';
import { TextArea } from '@astryxdesign/core/TextArea';
import { Button } from '@astryxdesign/core/Button';
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
    <Dialog
      isOpen
      onOpenChange={(open) => { if (!open) onClose(); }}
      purpose="form"
      width={440}
    >
      <DialogHeader title="Save to library" onOpenChange={(open) => { if (!open) onClose(); }} />
      <div className="settings-body">
        <div className="settings-section">
          {libraries.length > 0 && (
            <Selector
              label="Library"
              value={isNew ? '__new__' : targetSlug ?? ''}
              onChange={(v) => setTargetSlug(v === '__new__' ? null : v)}
              options={[
                ...libraries.map((l) => ({
                  value: l.slug,
                  label: `${l.library.name} (${l.library.patches.length})`,
                })),
                { type: 'divider' as const },
                { value: '__new__', label: '＋ New library…' },
              ]}
            />
          )}
          {(isNew || libraries.length === 0) && (
            <TextInput
              label="New library name"
              placeholder="New library name"
              value={newLibName}
              onChange={(v) => setNewLibName(v)}
            />
          )}
        </div>

        <div className="settings-section">
          <TextInput
            label="Patch name"
            placeholder="Patch name (shown in the menu)"
            value={entryName}
            onChange={(v) => setEntryName(v)}
          />
          <TextArea
            label="Description"
            isOptional
            placeholder="Description (optional)"
            value={entryDesc}
            onChange={(v) => setEntryDesc(v)}
            rows={3}
          />
        </div>

        {error && <div className="library-error">{error}</div>}
        {nodes.length === 0 && (
          <div className="library-empty">Current patch is empty — nothing to save.</div>
        )}

        <div className="settings-section library-save-actions">
          <Button variant="ghost" size="sm" label="Cancel" onClick={onClose} />
          <Button variant="primary" size="sm" label="Save" isDisabled={!canSave} onClick={onSave} />
        </div>
      </div>
    </Dialog>
  );
}
