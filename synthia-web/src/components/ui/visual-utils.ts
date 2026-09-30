/**
 * Pure helpers + constants shared by the visual primitives in
 * `./visual.tsx`. Split out so `visual.tsx` only exports
 * components, keeping the `react-refresh/only-export-components`
 * ESLint rule happy. Pages that want a util still import from
 * `./visual` — `visual.tsx` re-exports the public surface.
 */

/* ----------------------------------------------------------------
 * Entity theme map
 * ----------------------------------------------------------------
 *
 * Single source of truth for the colour and emoji glyph used
 * to visually identify each entity type. Pages look up their
 * theme here instead of repeating the literal colour code.
 *
 * The emoji is purely decorative — wrapped in `aria-hidden`
 * wherever it's used — and chosen to be readable on the
 * accent background. Pure unicode keeps the bundle free of
 * an icon-font dependency for what is essentially a small
 * marker.
 */

export type EntityKind = 'skill' | 'tool' | 'agent' | 'session';

export interface EntityTheme {
  /** Glyph shown in the icon tile and as a marker next to the title. */
  glyph: string;
  /** Tailwind-style modifier class applied to entity cards / heroes. */
  cardModifier: string;
  /** Tailwind-style modifier class applied to detail-page heroes. */
  heroModifier: string;
  /** Human label used in tooltips / aria-labels. */
  label: string;
}

export const ENTITY_THEMES: Record<EntityKind, EntityTheme> = {
  skill: {
    glyph: '🧠',
    cardModifier: 'nt-vis-card--skill',
    heroModifier: 'nt-vis-hero--skill',
    label: 'Skill',
  },
  tool: {
    glyph: '🛠️',
    cardModifier: 'nt-vis-card--tool',
    heroModifier: 'nt-vis-hero--tool',
    label: 'Tool',
  },
  agent: {
    glyph: '🤖',
    cardModifier: 'nt-vis-card--agent',
    heroModifier: 'nt-vis-hero--agent',
    label: 'Agent',
  },
  session: {
    glyph: '💬',
    cardModifier: 'nt-vis-card--session',
    heroModifier: 'nt-vis-hero--session',
    label: 'Session',
  },
};

/**
 * Pick the agent theme variant — built-in (protected) agents
 * get a distinct yellow/magenta accent so the user can tell
 * "system-provided" from "user-defined" at a glance. Kept as
 * a separate helper instead of a third entity kind so every
 * downstream type still treats the row as `EntityKind =
 * 'agent'`.
 */
export function agentTheme(protectedFlag: boolean): EntityTheme {
  if (protectedFlag) {
    return {
      glyph: '🛡️',
      cardModifier: 'nt-vis-card--agent-protected',
      heroModifier: 'nt-vis-hero--agent-protected',
      label: 'Built-in Agent',
    };
  }
  return ENTITY_THEMES.agent;
}

/* ----------------------------------------------------------------
 * Status pill tone
 * ----------------------------------------------------------------
 *
 * Map a free-form status string (typically the session
 * `status` field, but also tool `provenance`, agent `kind`)
 * to a tone + dot presence. Centralised so the colour
 * vocabulary stays consistent across pages — a "working"
 * session, an "in-flight" agent, and a "core" tool all
 * share the same green-with-dot, regardless of which page
 * renders them.
 */

export type PillTone =
  'blue' | 'cyan' | 'purple' | 'magenta' | 'yellow' | 'green' | 'red' | 'neutral' | 'ghost';

export function statusTone(status: string | undefined | null): PillTone {
  if (!status) return 'neutral';
  const s = status.toLowerCase();
  if (
    s === 'working' ||
    s === 'running' ||
    s === 'in_progress' ||
    s === 'pending' ||
    s === 'streaming'
  ) {
    return 'yellow';
  }
  if (s === 'completed' || s === 'done' || s === 'success' || s === 'idle' || s === 'ready') {
    return 'green';
  }
  if (s === 'failed' || s === 'error' || s === 'errored' || s === 'cancelled' || s === 'timeout') {
    return 'red';
  }
  if (s === 'paused' || s === 'waiting') {
    return 'cyan';
  }
  if (s === 'core') return 'blue';
  if (s === 'dynamic') return 'purple';
  if (s === 'react') return 'blue';
  if (s === 'pipeline') return 'purple';
  if (s === 'router') return 'cyan';
  return 'neutral';
}

/**
 * Whether the status should carry a leading dot. Static
 * descriptors (provenance / kind) don't pulse — running
 * states do.
 */
export function statusHasDot(status: string | undefined | null): boolean {
  if (!status) return false;
  const s = status.toLowerCase();
  return (
    s === 'working' ||
    s === 'running' ||
    s === 'in_progress' ||
    s === 'pending' ||
    s === 'streaming' ||
    s === 'completed' ||
    s === 'failed' ||
    s === 'cancelled'
  );
}

/* ----------------------------------------------------------------
 * Pretty timestamp helper
 * ----------------------------------------------------------------
 *
 * The detail pages pass through ISO timestamps that look
 * like `2026-01-01T12:34:56Z` — readable but not pretty.
 * Format them as `Jan 1, 2026 · 12:34` so the hero meta
 * cluster reads at a glance. Falls back to the raw string
 * when parsing fails (server may pass an unusual shape).
 */

const DATE_FMT = new Intl.DateTimeFormat(undefined, {
  year: 'numeric',
  month: 'short',
  day: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
});

export function formatTimestamp(iso: string | undefined | null): string | null {
  if (!iso) return null;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return DATE_FMT.format(d);
}

/* ----------------------------------------------------------------
 * Re-export `shortId` so pages only need one import
 * ---------------------------------------------------------------- */

export { shortId } from '../../lib/short-id';

// Re-export React types so consumers can `import type` from the
// same module without pulling in `react` separately.
export type { ReactNode, CSSProperties } from 'react';
