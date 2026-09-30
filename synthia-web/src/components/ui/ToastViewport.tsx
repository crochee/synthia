import type { Toast } from '../../hooks/useToast';

interface ToastViewportProps {
  toasts: Toast[];
  onDismiss: (id: string) => void;
}

/**
 * Fixed-position stack at the top-right. Renders only the
 * queue — the toast content is the consumer's responsibility
 * via the `useToast()` hook. Kept lightweight (no animations)
 * to avoid the `prefers-reduced-motion` dance.
 */
export function ToastViewport({ toasts, onDismiss }: ToastViewportProps) {
  if (toasts.length === 0) return null;
  return (
    <div
      role="region"
      aria-label="Notifications"
      // Note: deliberately *no* `aria-live` on the region. The
      // individual `<ToastItem>`s carry `role="alert"` /
      // `role="status"` which already triggers the right
      // announcement level. Adding a parent `aria-live` to a
      // container with role-alert descendants is an a11y
      // anti-pattern: the parent live region waits for the user
      // to be idle before announcing, which can swallow the
      // urgent `role="alert"` queue entirely.
      style={{
        position: 'fixed',
        top: 16,
        right: 16,
        zIndex: 'var(--z-toast)',
        display: 'flex',
        flexDirection: 'column',
        gap: 'var(--spacing-sm)',
        maxWidth: 380,
        pointerEvents: 'none',
      }}
    >
      {toasts.map((t) => (
        <ToastItem key={t.id} toast={t} onDismiss={onDismiss} />
      ))}
    </div>
  );
}

const VARIANT_COLOR: Record<Toast['variant'], { bg: string; border: string; fg: string }> = {
  info: { bg: 'var(--bg-secondary)', border: 'var(--accent-primary)', fg: 'var(--text-primary)' },
  success: {
    bg: 'var(--bg-secondary)',
    border: 'var(--accent-success)',
    fg: 'var(--text-primary)',
  },
  warning: { bg: 'var(--bg-secondary)', border: 'var(--accent-yellow)', fg: 'var(--text-primary)' },
  error: { bg: 'var(--bg-secondary)', border: 'var(--accent-red)', fg: 'var(--text-primary)' },
};

function ToastItem({ toast, onDismiss }: { toast: Toast; onDismiss: (id: string) => void }) {
  const colors = VARIANT_COLOR[toast.variant];
  return (
    <div
      role={toast.variant === 'error' || toast.variant === 'warning' ? 'alert' : 'status'}
      style={{
        pointerEvents: 'auto',
        padding: 'var(--spacing-sm) var(--spacing-md)',
        background: colors.bg,
        border: '1px solid var(--border-subtle)',
        borderLeft: `3px solid ${colors.border}`,
        borderRadius: 'var(--radius-md)',
        boxShadow: 'var(--shadow-md)',
        fontFamily: 'var(--font-sans)',
        fontSize: 'var(--fs-sm)',
        color: colors.fg,
        display: 'flex',
        alignItems: 'center',
        gap: 'var(--spacing-sm)',
      }}
    >
      <span style={{ flex: 1 }}>{toast.message}</span>
      {toast.action && (
        <button
          type="button"
          onClick={() => {
            toast.action!.onClick();
            onDismiss(toast.id);
          }}
          style={{
            border: 'none',
            background: 'transparent',
            color: 'var(--accent-primary)',
            cursor: 'pointer',
            fontWeight: 'var(--fw-medium)',
            fontFamily: 'inherit',
            fontSize: 'inherit',
            padding: '0 var(--spacing-xs)',
          }}
        >
          {toast.action.label}
        </button>
      )}
      <button
        type="button"
        aria-label="Dismiss notification"
        onClick={() => onDismiss(toast.id)}
        style={{
          border: 'none',
          background: 'transparent',
          color: 'var(--text-muted)',
          cursor: 'pointer',
          fontSize: 'var(--fs-md)',
          lineHeight: 1,
          padding: 0,
        }}
      >
        ×
      </button>
    </div>
  );
}
