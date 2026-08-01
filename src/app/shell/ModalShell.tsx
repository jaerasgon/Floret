import type { ReactNode } from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import IconButton from '@mui/material/IconButton';
import Typography from '@mui/material/Typography';
import { X } from 'lucide-react';

/**
 * Shared Material 3 modal chrome — a MUI Dialog with a titled header, optional
 * subtitle, and a close affordance. Replaces Astryx's Dialog + DialogHeader so
 * every Floret modal reads as one Material surface. Backdrop click and Esc both
 * close (MUI's onClose), matching the old onOpenChange(false) behaviour. The
 * body is passed as children and keeps its own layout classNames.
 */
export function ModalShell({
  title,
  subtitle,
  width = 440,
  onClose,
  children,
}: {
  title: string;
  subtitle?: string;
  width?: number;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <Dialog open onClose={onClose} slotProps={{ paper: { sx: { width, maxWidth: '92vw' } } }}>
      <DialogTitle
        component="div"
        sx={{ display: 'flex', alignItems: 'flex-start', gap: 1, px: 3, pt: 2.5, pb: subtitle ? 1.5 : 1 }}
      >
        <div style={{ flex: 1, minWidth: 0 }}>
          <Typography variant="subtitle1" component="h2" sx={{ fontWeight: 600, lineHeight: 1.35 }}>
            {title}
          </Typography>
          {subtitle && (
            <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5, lineHeight: 1.45 }}>
              {subtitle}
            </Typography>
          )}
        </div>
        <IconButton
          size="small"
          aria-label="Close"
          onClick={onClose}
          sx={{ mt: -0.5, mr: -0.5, color: 'text.secondary' }}
        >
          <X size={16} strokeWidth={1.8} />
        </IconButton>
      </DialogTitle>
      {children}
    </Dialog>
  );
}
