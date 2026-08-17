import type { Theme } from '../../hooks/useTheme';
import { useTheme } from '../../hooks/useTheme';

const OPTIONS: ReadonlyArray<{ value: Theme; label: string; glyph: string }> = [
  { value: 'light', label: 'Light', glyph: '☀' },
  { value: 'system', label: 'System', glyph: '◐' },
  { value: 'dark', label: 'Dark', glyph: '☾' },
];

/**
 * Three-state segmented control for theme switching. Lives in
 * the Header. The `system` choice follows the OS preference
 * via `prefers-color-scheme`; the other two stamp a
 * `data-theme` attribute on `<html>` so the cascade in
 * `tokens.css` resolves the dark/light token set.
 *
 * The control itself never re-renders the rest of the page —
 * the change is purely CSS-driven once `data-theme` is set.
 */
export function ThemeToggle() {
  const { theme, setTheme } = useTheme();
  return (
    <div
      role="radiogroup"
      aria-label="Theme"
      style={{
        display: 'inline-flex',
        border: '1px solid var(--border-subtle)',
        borderRadius: 'var(--radius-md)',
        overflow: 'hidden',
        background: 'var(--bg-secondary)',
      }}
    >
      {OPTIONS.map((opt) => {
        const selected = theme === opt.value;
        return (
          <button
            key={opt.value}
            type="button"
            role="radio"
            aria-checked={selected}
            aria-label={`${opt.label} theme`}
            data-testid={`theme-${opt.value}`}
            onClick={() => setTheme(opt.value)}
            style={{
              padding: '4px 8px',
              border: 'none',
              background: selected ? 'var(--accent-primary)' : 'transparent',
              color: selected ? 'var(--text-inverse)' : 'var(--text-secondary)',
              cursor: 'pointer',
              fontFamily: 'inherit',
              fontSize: 'var(--fs-sm)',
              display: 'inline-flex',
              alignItems: 'center',
              gap: '4px',
              transition: 'background-color var(--transition-fast)',
            }}
          >
            <span aria-hidden style={{ fontSize: 'var(--fs-md)' }}>
              {opt.glyph}
            </span>
            <span>{opt.label}</span>
          </button>
        );
      })}
    </div>
  );
}