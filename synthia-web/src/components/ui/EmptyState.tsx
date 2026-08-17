import type { ReactNode } from 'react';

interface EmptyStateProps {
  /** Optional emoji or single-character glyph to anchor the message. */
  icon?: string;
  title: string;
  description?: ReactNode;
  /** Optional CTA — e.g. a "Reload" button for a failed list. */
  action?: ReactNode;
  /** ARIA-friendly role. Defaults to `status` (informational). */
  role?: 'status' | 'region';
  testId?: string;
}

/**
 * Render an empty / failed / no-results state in a consistent
 * shape. Centralises the visual treatment so every list page
 * (Tools / Skills / Agents / Tasks) gives the user the same
 * cue when there's nothing to show.
 *
 * The component is intentionally pure — no Radix dependency —
 * so it can sit inside a Card, a section, or a flex column
 * without forcing a particular layout.
 */
export function EmptyState({
  icon,
  title,
  description,
  action,
  role = 'status',
  testId,
}: EmptyStateProps) {
  return (
    <div
      role={role}
      data-testid={testId}
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 'var(--spacing-sm)',
        padding: 'var(--spacing-xl) var(--spacing-md)',
        textAlign: 'center',
        color: 'var(--text-secondary)',
        fontFamily: 'var(--font-sans)',
      }}
    >
      {icon && (
        <span
          aria-hidden
          style={{ fontSize: 'var(--fs-2xl)', lineHeight: 1, opacity: 0.6 }}
        >
          {icon}
        </span>
      )}
      <strong
        style={{
          color: 'var(--text-primary)',
          fontSize: 'var(--fs-md)',
          fontWeight: 'var(--fw-medium)',
        }}
      >
        {title}
      </strong>
      {description && (
        <span style={{ fontSize: 'var(--fs-sm)', maxWidth: 420 }}>{description}</span>
      )}
      {action && <div style={{ marginTop: 'var(--spacing-xs)' }}>{action}</div>}
    </div>
  );
}