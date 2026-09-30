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
  /** Extra class on the panel itself, for callers whose dialog is
   *  not a centred form (the command palette anchors to the top
   *  of the viewport and wants a wider panel). The panel is the
   *  only element a caller can restyle without reaching into
   *  `.nt-modal__*` from another file. */
  panelClassName?: string;
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
export function Modal({
  open,
  onClose,
  title,
  children,
  footer,
  titleId,
  testId,
  panelClassName,
}: ModalProps) {
  const panelRef = useRef<HTMLDivElement | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);
  // Latest `onClose` without making it an effect dependency. Callers
  // pass an inline closure (`onClose={() => setOpen(false)}`), so its
  // identity changes on every render — and an effect keyed on it would
  // tear down and re-run on every keystroke in a form inside the
  // dialog. The teardown restores focus to whatever was focused when
  // the dialog opened, which yanks the caret out of the input the user
  // is typing into: only the first character of each field survived.
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  // Keyed on `open` alone: mounted once per open/close transition, so
  // focus is captured and restored exactly once each.
  useEffect(() => {
    if (!open) return;
    previousFocusRef.current = document.activeElement as HTMLElement | null;
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    // Focus the panel synchronously rather than in
    // `requestAnimationFrame`: a callback scheduled there never runs in
    // a background tab, which left the dialog focusable-but-unfocused.
    // Selecting the panel's first control when it has one keeps the
    // dialog usable from the keyboard immediately; otherwise the panel
    // itself takes focus so `Tab` still enters the dialog.
    const firstControl = panelRef.current?.querySelector<HTMLElement>(
      'input:not([type="hidden"]), select, textarea',
    );
    (firstControl ?? panelRef.current)?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onCloseRef.current();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('keydown', onKey);
      document.body.style.overflow = prevOverflow;
      previousFocusRef.current?.focus?.();
    };
  }, [open]);

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
        className={`nt-modal__panel${panelClassName ? ` ${panelClassName}` : ''}`}
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
