import { useEffect, useMemo, useState } from 'react';
import Card from '@mui/material/Card';
import CardActionArea from '@mui/material/CardActionArea';
import Button from '@mui/material/Button';
import Box from '@mui/material/Box';
import { useEditor } from '../../editor/editor-state';
import { paramStore } from '../../runtime/param-store';
import { history } from '../../editor/history';
import { SAMPLES } from '../../persistence/samples';
import { ModalShell } from './ModalShell';

/**
 * Welcome card shown on first launch. Points new users at the Tutorial
 * track and exposes one-click loaders for the first few lessons.
 *
 * Visibility:
 *   - shown when the patch is the empty starter AND the user hasn't
 *     dismissed it before (localStorage flag).
 *   - dismissed permanently when the user clicks "Skip" / loads a sample,
 *     or temporarily when the user adds their first node.
 */

const LS_KEY = 'floret.welcomeDismissed.v1';

const FEATURED_IDS = [
  'tut-01-hello-rect',
  'tut-03-animate',
  'tut-07-grid',
  'tut-50-showcase',
];

/** Small keycap (replaces Astryx's Kbd). */
function Kbd({ keys }: { keys: string }) {
  return (
    <Box
      component="kbd"
      sx={{
        display: 'inline-block',
        px: 0.75,
        py: 0.125,
        fontFamily: 'var(--font-mono)',
        fontSize: 11,
        lineHeight: 1.5,
        color: 'text.secondary',
        border: '1px solid var(--md-sys-color-outline-variant)',
        borderRadius: 1,
        backgroundColor: 'var(--md-sys-color-surface-container-high)',
      }}
    >
      {keys}
    </Box>
  );
}

export function WelcomeModal() {
  const loadPatch = useEditor((s) => s.loadPatch);
  const nodes = useEditor((s) => s.nodes);
  const [dismissed, setDismissed] = useState<boolean>(() => {
    try { return localStorage.getItem(LS_KEY) === '1'; } catch { return false; }
  });

  // Treat "no nodes" or "just the starter (one rect + one renderer)" as
  // empty for the purpose of showing the welcome.
  const isEmpty = useMemo(() => {
    if (nodes.length === 0) return true;
    if (nodes.length <= 3) {
      // Pure starter patches only contain a draw/rect → renderer (with
      // maybe a value/number). If everything is in those, count as empty.
      const allStarter = nodes.every((n) =>
        ['draw/rect', 'render/renderer', 'value/number'].includes(n.data.defType),
      );
      if (allStarter) return true;
    }
    return false;
  }, [nodes]);

  // Auto-dismiss when the user has built something real.
  useEffect(() => {
    if (!isEmpty && !dismissed) {
      // Soft dismiss for this session — don't persist, so users get the
      // welcome again next time they open an empty canvas. We do mark
      // them dismissed in component state so the modal hides immediately.
      setDismissed(true);
    }
  }, [isEmpty, dismissed]);

  const dismissPermanent = () => {
    setDismissed(true);
    try { localStorage.setItem(LS_KEY, '1'); } catch { /* ignore */ }
  };

  const loadSample = (id: string) => {
    const sample = SAMPLES.find((s) => s.id === id);
    if (!sample) return;
    const { nodes: ns, edges: es } = sample.build();
    paramStore.replaceAll(ns.map((n) => [n.id, n.data.params] as const));
    loadPatch(ns, es);
    history.reset();
    dismissPermanent();
  };

  if (dismissed || !isEmpty) return null;

  const featured = FEATURED_IDS
    .map((id) => SAMPLES.find((s) => s.id === id))
    .filter(<T,>(s: T | undefined): s is T => Boolean(s));

  return (
    <ModalShell
      title="Welcome to Floret"
      subtitle="A node-based creative tool — wire shapes, sounds, and data together to make visual sketches."
      width={560}
      onClose={dismissPermanent}
    >
      <div className="welcome-body">
        <h3 className="welcome-section-title">Start here</h3>
        <p className="welcome-section-lede">
          The Tutorial tab in the Samples menu has 50 lessons that build
          up from a single rectangle to full audio-reactive compositions.
        </p>
        <div className="welcome-featured">
          {featured.map((s) => (
            <Card key={s.id} variant="outlined" sx={{ backgroundColor: 'var(--md-sys-color-surface-container-highest)' }}>
              <CardActionArea onClick={() => loadSample(s.id)} sx={{ p: 1.5, display: 'block', height: '100%' }}>
                <span className="welcome-sample-name">{s.name}</span>
                <span className="welcome-sample-desc">{s.description}</span>
              </CardActionArea>
            </Card>
          ))}
        </div>
        <div className="welcome-tips">
          <p>
            <strong>Editor basics</strong> — drag from output pins (right) to
            input pins (left). Double-click empty space to open the node
            search. Press <Kbd keys="tab" /> anywhere to do the same.
          </p>
          <p>
            <strong>See what's happening</strong> — hover any pin for its
            live value. Select a node to read its docs in the Help panel
            on the right.
          </p>
        </div>
        <div className="welcome-actions">
          <Button variant="text" size="small" color="inherit" onClick={dismissPermanent}>
            Skip — I'll explore on my own
          </Button>
        </div>
      </div>
    </ModalShell>
  );
}
