import { useEffect, useRef, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { X } from 'lucide-react';

/**
 * Shared native modal — a dimmed backdrop and a centered card. Closes on Esc
 * or a backdrop click; focus moves into the card on open and restores on close.
 * Replaces the Astryx Dialog so the chrome has no design-system dependency.
 */
export function Modal({
  onClose,
  title,
  subtitle,
  width = 440,
  children,
}: {
  onClose: () => void;
  title: string;
  subtitle?: string;
  width?: number;
  children: ReactNode;
}) {
  const cardRef = useRef<HTMLDivElement | null>(null);
  const restoreRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    restoreRef.current = document.activeElement as HTMLElement | null;
    cardRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.stopPropagation(); onClose(); }
    };
    document.addEventListener('keydown', onKey, true);
    return () => {
      document.removeEventListener('keydown', onKey, true);
      restoreRef.current?.focus?.();
    };
  }, [onClose]);

  return createPortal(
    <div className="modal-backdrop" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div
        className="modal-card"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        ref={cardRef}
        style={{ width }}
      >
        <header className="modal-header">
          <div className="modal-header-text">
            <h2 className="modal-title">{title}</h2>
            {subtitle && <p className="modal-subtitle">{subtitle}</p>}
          </div>
          <button type="button" className="modal-close" aria-label="Close" onClick={onClose}>
            <X size={16} strokeWidth={2} />
          </button>
        </header>
        <div className="modal-body">{children}</div>
      </div>
    </div>,
    document.body,
  );
}

/** Bare native toggle switch, styled by `.modal-switch`. */
export function Switch({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={`modal-switch${checked ? ' on' : ''}`}
      onClick={() => onChange(!checked)}
    >
      <span className="modal-switch-thumb" />
    </button>
  );
}

/** Labelled switch row (label + description on the left, switch on the right). */
export function Toggle({
  checked,
  onChange,
  label,
  description,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  description?: string;
}) {
  return (
    <label className="modal-switch-row">
      <span className="modal-switch-text">
        <span className="modal-switch-label">{label}</span>
        {description && <span className="modal-switch-desc">{description}</span>}
      </span>
      <Switch checked={checked} onChange={onChange} label={label} />
    </label>
  );
}
