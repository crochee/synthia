import { useEffect, useRef, type ReactNode } from 'react';

export interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  children: ReactNode;
  /** Optional footer (typically a row of action buttons). */
  footer?: ReactNode;
  /** ARIA-friendly identifier for the panel; used by `aria-labelledby`. */
  titleId?: string;
  testId?: string;
}

/**
 * Lightweight modal dialog used for short, focused forms (e.g.
 * "Register Agent"). The list pages stay list-first — a
 * "Create" button in the toolbar opens this dialog rather than
 * pinning a multi-field form above the list.
 *
 * Implementation notes:
 *
 * - Pure DOM + CSS. No Radix Dialog dependency is required for
 *   the simple modal flows the app uses today; introducing one
 *   would force us to keep an extra primitive in sync with the
 *   design tokens for very little benefit.
 * - `Escape` and backdrop click both close the dialog. Pressing
 *   `Escape` is a hard contract — without it, keyboard users
 *   would have to find the close button to escape the modal.
 * - Focus is moved into the dialog on open and restored to the
 *   previously-focused element on close. Without focus
 *   management, the dialog traps keyboard focus on the body and
 *   screen-reader users lose context.
 * - `useRef` + `tabIndex` makes the panel itself focusable so
 *   the initial focus lands inside the dialog content (rather
 *   than on the document body, which would skip the first
 *   tabbable element inside the dialog).
 * - Scroll on `<body>` is locked while the dialog is open so
 *   scrolling the underlying list doesn't bleed through.
 */
export function Modal({ open, onClose, title, children, footer, titleId, testId }: ModalProps) {
  const panelRef = useRef<HTMLDivElement | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return;
    previousFocusRef.current = document.activeElement as HTMLElement | null;
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    // Move focus into the panel on the next frame so the
    // dialog content is reachable via Tab before the user
    // starts navigating.
    const id = requestAnimationFrame(() => {
      panelRef.current?.focus();
    });
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => {
      cancelAnimationFrame(id);
      document.removeEventListener('keydown', onKey);
      document.body.style.overflow = prevOverflow;
      previousFocusRef.current?.focus?.();
    };
  }, [open, onClose]);

  if (!open) return null;

  const resolvedTitleId = titleId ?? 'nt-modal-title';

  return (
    <div
      className="nt-modal__backdrop"
      onClick={onClose}
      data-testid={testId ? `${testId}-backdrop` : 'modal-backdrop'}
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={resolvedTitleId}
        tabIndex={-1}
        className="nt-modal__panel"
        data-testid={testId}
        // Stop click propagation so clicking inside the panel
        // doesn't bubble to the backdrop and trigger `onClose`.
        onClick={(e) => e.stopPropagation()}
      >
        <div className="nt-modal__header">
          <h2 id={resolvedTitleId} className="nt-modal__title">
            {title}
          </h2>
          <button
            type="button"
            aria-label="Close"
            className="nt-modal__close"
            onClick={onClose}
            data-testid={testId ? `${testId}-close` : 'modal-close'}
          >
            ×
          </button>
        </div>
        <div className="nt-modal__body">{children}</div>
        {footer && <div className="nt-modal__footer">{footer}</div>}
      </div>
    </div>
  );
}
