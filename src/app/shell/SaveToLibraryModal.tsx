import { useState } from 'react';
import TextField from '@mui/material/TextField';
import MenuItem from '@mui/material/MenuItem';
import Divider from '@mui/material/Divider';
import Button from '@mui/material/Button';
import Box from '@mui/material/Box';
import { useEditor } from '../../editor/editor-state';
import { serializePatch } from '../../persistence/serialize';
import {
  createLibrary,
  listLibraries,
  upsertLibraryEntry,
} from '../../persistence/library';
import { ModalShell } from './ModalShell';

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
    <ModalShell title="Save to library" width={440} onClose={onClose}>
      <div className="settings-body">
        <div className="settings-section">
          {libraries.length > 0 && (
            <TextField
              select
              label="Library"
              size="small"
              fullWidth
              margin="dense"
              value={isNew ? '__new__' : targetSlug ?? ''}
              onChange={(e) => setTargetSlug(e.target.value === '__new__' ? null : e.target.value)}
            >
              {libraries.map((l) => (
                <MenuItem key={l.slug} value={l.slug}>
                  {`${l.library.name} (${l.library.patches.length})`}
                </MenuItem>
              ))}
              <Divider />
              <MenuItem value="__new__">＋ New library…</MenuItem>
            </TextField>
          )}
          {(isNew || libraries.length === 0) && (
            <TextField
              label="New library name"
              placeholder="New library name"
              size="small"
              fullWidth
              margin="dense"
              value={newLibName}
              onChange={(e) => setNewLibName(e.target.value)}
            />
          )}
        </div>

        <div className="settings-section">
          <TextField
            label="Patch name"
            placeholder="Patch name (shown in the menu)"
            size="small"
            fullWidth
            margin="dense"
            value={entryName}
            onChange={(e) => setEntryName(e.target.value)}
          />
          <TextField
            label="Description"
            placeholder="Description (optional)"
            size="small"
            fullWidth
            margin="dense"
            multiline
            minRows={3}
            value={entryDesc}
            onChange={(e) => setEntryDesc(e.target.value)}
          />
        </div>

        {error && <div className="library-error">{error}</div>}
        {nodes.length === 0 && (
          <div className="library-empty">Current patch is empty — nothing to save.</div>
        )}

        <Box className="settings-section library-save-actions" sx={{ display: 'flex', gap: 1, justifyContent: 'flex-end' }}>
          <Button variant="text" size="small" color="inherit" onClick={onClose}>Cancel</Button>
          <Button variant="contained" size="small" disabled={!canSave} onClick={onSave}>Save</Button>
        </Box>
      </div>
    </ModalShell>
  );
}
