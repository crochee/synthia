import { useEffect, type ReactNode } from 'react';
import './Modal.css';

export interface ModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  footer?: ReactNode;
  width?: 'sm' | 'md' | 'lg';
}

/**
 * Neon Terminal-styled modal dialog.
 * Includes backdrop, escape-to-close, and click-outside-to-close.
 */
export function Modal({ isOpen, onClose, title, children, footer, width = 'md' }: ModalProps) {
  useEffect(() => {
    if (!isOpen) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return (
    <div className="nt-modal-backdrop" onClick={onClose} role="presentation">
      <div
        className={`nt-modal nt-modal--${width}`}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label={title}
      >
        <div className="nt-modal__header">
          <h2 className="nt-modal__title">{title}</h2>
          <button className="nt-modal__close" onClick={onClose} aria-label="Close" type="button">
            ×
          </button>
        </div>
        <div className="nt-modal__body">{children}</div>
        {footer && <div className="nt-modal__footer">{footer}</div>}
      </div>
    </div>
  );
}
