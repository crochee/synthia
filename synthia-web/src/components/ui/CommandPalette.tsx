import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { useNavigate } from 'react-router-dom';

import { Modal } from './Modal';
import { Input } from './Input';
import { useCursorList } from '../../hooks/useCursorList';
import { isEditableTarget } from '../../hooks/useKeyboardShortcuts';
import { shortId } from '../../lib/short-id';
import { api } from '../../api/client';
import type { AgentDetail, List, SearchHit, SessionSummary } from '../../api/types';
import './CommandPalette.css';

/**
 * The five top-level destinations, mirroring `Sidebar`'s list. The
 * `shortcut` is the same `g <key>` accelerator the sidebar prints,
 * so the palette teaches the muscle memory instead of competing
 * with it.
 */
const NAV_COMMANDS: ReadonlyArray<{ path: string; label: string; shortcut: string }> = [
  { path: '/chat', label: 'Go to Chat', shortcut: 'g c' },
  { path: '/tools', label: 'Go to Tools', shortcut: 'g t' },
  { path: '/agents', label: 'Go to Agents', shortcut: 'g g' },
  { path: '/skills', label: 'Go to Skills', shortcut: 'g k' },
  { path: '/sessions', label: 'Go to Sessions', shortcut: 'g s' },
  { path: '/schedules', label: 'Go to Schedules', shortcut: 'g h' },
  { path: '/workflows', label: 'Go to Workflows', shortcut: 'g w' },
  { path: '/evals', label: 'Go to Evals', shortcut: 'g e' },
  { path: '/tasks', label: 'Go to Tasks', shortcut: 'g d' },
];

/** Group order is fixed (Navigate → server-ranked domain groups)
 *  so a strong session match cannot interleave its way between
 *  the nav rows: grouped results stay scannable, and only the
 *  ranking *inside* a group follows the match score. */
const GROUP_NAVIGATE = 'Navigate';
const GROUP_SESSIONS = 'Sessions';
const GROUP_AGENTS = 'Agents';
const GROUP_TOOLS = 'Tools';
const GROUP_SKILLS = 'Skills';
const GROUP_MEMORY = 'Memory';

/** Rows per group before the user types — the palette is a jump
 *  list, not a session browser. */
const IDLE_GROUP_LIMIT = 6;
/** Rows per group once there is a query: enough to disambiguate a
 *  match, still short enough to scan without scrolling far. */
const SEARCH_GROUP_LIMIT = 20;

interface Command {
  id: string;
  group: string;
  label: string;
  detail?: string;
  /** Text the matcher may look at that is not rendered (opaque
   *  ids the user may paste from a log, status words, …). */
  keywords?: string;
  /** Right-aligned accelerator hint. */
  shortcut?: string;
  /** Set on server-ranked hits: the backend already scored them
   *  (BM25 + vector, not subsequence), so the local fuzzy filter
   *  must pass them through instead of re-matching. */
  serverRanked?: boolean;
  run: () => void;
}

/**
 * Subsequence match with a small positional bonus, in the spirit of
 * the fuzzy finders every editor ships.
 *
 * Returns `null` when `query` is not a subsequence of `text`;
 * otherwise a score where higher is a better match. The bonuses
 * are what make "ch" prefer "Chat" (matches at the start) over
 * "Go to Skills" (a scattered hit). `text` is matched
 * case-insensitively; the length penalty in the final score breaks
 * ties in favour of the shorter, more likely candidate.
 */
function fuzzyScore(text: string, query: string): number | null {
  const haystack = text.toLowerCase();
  let cursor = 0;
  let score = 0;
  let previousIndex = -2;
  for (const character of query) {
    const index = haystack.indexOf(character, cursor);
    if (index === -1) return null;
    if (index === 0) score += 4;
    else if (index === previousIndex + 1) score += 2;
    previousIndex = index;
    cursor = index + 1;
  }
  return score - haystack.length / 100;
}

function sessionCommands(
  sessions: ReadonlyArray<SessionSummary>,
  navigate: (path: string) => void,
): Command[] {
  return sessions.map((session) => ({
    id: `session:${session.id}`,
    group: GROUP_SESSIONS,
    label: shortId(session.id),
    detail: session.status,
    keywords: session.id,
    shortcut: '⏎ open',
    run: () => navigate(`/sessions/${encodeURIComponent(session.id)}`),
  }));
}
function agentCommands(
  agents: ReadonlyArray<AgentDetail>,
  navigate: (path: string) => void,
): Command[] {
  return agents.map((agent) => ({
    id: `agent:${agent.name}`,
    group: GROUP_AGENTS,
    label: agent.name,
    detail: agent.description,
    keywords: `${agent.kind} ${agent.domain ?? ''} ${agent.owner ?? ''}`,
    shortcut: '⏎ open',
    run: () => navigate(`/agents/${encodeURIComponent(agent.name)}`),
  }));
}

/** Where a server search hit navigates — same mapping the
 *  Sessions page's unified card uses. */
function searchHitPath(hit: SearchHit): string | null {
  switch (hit.domain) {
    case 'session':
      return `/sessions/${encodeURIComponent(hit.id)}`;
    case 'skill':
    case 'agent': {
      const name = hit.id.split(':')[1];
      if (!name) return null;
      return `/${hit.domain === 'skill' ? 'skills' : 'agents'}/${encodeURIComponent(name)}`;
    }
    case 'tool':
    case 'mcp':
      return `/tools/${encodeURIComponent(hit.id)}`;
    default:
      return null;
  }
}

/** Map server hits to palette commands, grouped by domain. The
 *  hits arrive best-first from `GET /api/v1/search`; each keeps
 *  its score so the in-group order matches the backend's
 *  ranking.
 *
 *  Domains without a page to land on (memory entries) are
 *  omitted: every palette row runs a navigation, so a row that
 *  could not take the user anywhere would be a dead affordance.
 *  Those hits stay reachable from the Sessions page's search
 *  card, which renders them inline. */
function searchHitCommands(
  hits: ReadonlyArray<SearchHit>,
  navigate: (path: string) => void,
): Command[] {
  return hits.flatMap((hit) => {
    const path = searchHitPath(hit);
    if (!path) return [];
    const group =
      hit.domain === 'session'
        ? GROUP_SESSIONS
        : hit.domain === 'agent'
          ? GROUP_AGENTS
          : hit.domain === 'skill'
            ? GROUP_SKILLS
            : hit.domain === 'memory'
              ? GROUP_MEMORY
              : GROUP_TOOLS;
    return [
      {
        id: `hit:${hit.domain}:${hit.id}`,
        group,
        label: hit.title || hit.id,
        detail: hit.preview ?? hit.id,
        keywords: hit.id,
        shortcut: '⏎ open',
        serverRanked: true,
        run: () => navigate(path),
      },
    ];
  });
}

/**
 * Global command palette (`Ctrl/Cmd+K`).
 *
 * Bounded deliberately: it navigates — nothing here mutates state,
 * submits a prompt, or deletes anything. That keeps a misfired
 * keystroke cheap, which is the only reason a palette this easy to
 * open is safe to have.
 *
 * The open state and the shortcut both live in this component so
 * the whole feature is one mount in `App.tsx`. The body is a
 * *child*, which is what gives the data-fetch contract for free:
 * `<Modal>` renders nothing while closed, so the body's
 * `useCursorList` calls mount on open and unmount on close — the
 * hook's own cleanup aborts the in-flight request, and nothing can
 * paint a stale result into a closed palette.
 */
export function CommandPalette() {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    function handleKeyDown(event: globalThis.KeyboardEvent): void {
      if (!event.metaKey && !event.ctrlKey) return;
      if (event.altKey || event.shiftKey) return;
      if (event.key.toLowerCase() !== 'k') return;
      // Reusing the shared guard: `Ctrl+K` in a text field is
      // "kill to end of line" on macOS and "jump the caret" in
      // several Linux editors. Stealing it would be a data-loss bug.
      if (isEditableTarget(event.target)) return;
      event.preventDefault();
      setOpen((value) => !value);
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  const close = useCallback(() => setOpen(false), []);

  return (
    <Modal
      open={open}
      onClose={close}
      title="Command palette"
      titleId="nt-command-palette-title"
      testId="command-palette"
      panelClassName="nt-palette"
    >
      <PaletteBody onClose={close} />
    </Modal>
  );
}

/**
 * Palette contents. Mounted only while the dialog is open — see the
 * comment on `CommandPalette` for why that is load-bearing.
 */
function PaletteBody({ onClose }: { onClose: () => void }) {
  const navigate = useNavigate();
  const [query, setQuery] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);

  // Sessions and agents are fetched here, not in the parent: this
  // component does not exist until the user opens the palette, and
  // it stops existing when they close it. `useCursorList` aborts
  // whatever is in flight on unmount, so a fast open/close cannot
  // leak a request or land a response after the close.
  const sessions = useCursorList<SessionSummary>('/api/v1/sessions');
  const agents = useCursorList<AgentDetail>('/api/v1/agents');

  // Server-side cross-domain search: once the user types, the
  // palette queries `GET /api/v1/search` (the same engine the
  // agent's `search` tool uses) instead of fuzzy-filtering the
  // two idle lists — so tools, skills, memory and MCP tools
  // become jump targets too. Debounced; the previous request is
  // aborted so a fast typist cannot land stale hits.
  const [serverHits, setServerHits] = useState<SearchHit[]>([]);
  const searchAbortRef = useRef<AbortController | null>(null);
  const trimmedQuery = query.trim();
  useEffect(() => {
    if (!trimmedQuery) {
      if (searchAbortRef.current) searchAbortRef.current.abort();
      setServerHits([]);
      return;
    }
    const timer = setTimeout(() => {
      if (searchAbortRef.current) searchAbortRef.current.abort();
      const controller = new AbortController();
      searchAbortRef.current = controller;
      api
        .get<List<SearchHit>>(
          `/api/v1/search?q=${encodeURIComponent(trimmedQuery)}&limit=10`,
          controller.signal,
        )
        .then((response) => {
          if (searchAbortRef.current === controller) {
            setServerHits(response.data);
          }
        })
        .catch(() => {
          // Aborts and transient errors keep the previous hits;
          // the palette stays usable offline via the local lists.
        });
    }, 200);
    return () => clearTimeout(timer);
  }, [trimmedQuery]);
  useEffect(() => {
    return () => {
      if (searchAbortRef.current) searchAbortRef.current.abort();
    };
  }, []);

  const commands = useMemo<Command[]>(
    () => [
      ...NAV_COMMANDS.map((item) => ({
        id: `nav:${item.path}`,
        group: GROUP_NAVIGATE,
        label: item.label,
        detail: item.path,
        shortcut: item.shortcut,
        run: () => navigate(item.path),
      })),
      ...(trimmedQuery
        ? searchHitCommands(serverHits, navigate)
        : [...sessionCommands(sessions.items, navigate), ...agentCommands(agents.items, navigate)]),
    ],
    [agents.items, navigate, serverHits, sessions.items, trimmedQuery],
  );

  const groups = useMemo(() => {
    const trimmed = query.trim().toLowerCase();
    const limit = trimmed ? SEARCH_GROUP_LIMIT : IDLE_GROUP_LIMIT;
    const buckets = new Map<string, Array<{ score: number; command: Command }>>();
    for (const command of commands) {
      let score = 0;
      if (trimmed) {
        // Server-ranked hits already matched (BM25 + vector over
        // the backend's catalogs) — a local subsequence filter
        // would drop semantic matches the user asked for.
        if (command.serverRanked) {
          score = 1;
        } else {
          const matched = fuzzyScore(
            `${command.label} ${command.detail ?? ''} ${command.keywords ?? ''}`,
            trimmed,
          );
          if (matched === null) continue;
          score = matched;
        }
      }
      const bucket = buckets.get(command.group);
      if (bucket) bucket.push({ score, command });
      else buckets.set(command.group, [{ score, command }]);
    }
    return Array.from(buckets, ([label, entries]) => ({
      label,
      commands: entries
        .slice()
        // `Array.prototype.sort` is stable, so equal scores keep the
        // server's ordering (best-first from `/api/v1/search`).
        .sort((a, b) => b.score - a.score)
        .slice(0, limit)
        .map((entry) => entry.command),
    }));
  }, [commands, query]);

  // Flat index → command. The rendered option ids and the
  // `aria-activedescendant` value both come from this list, so the
  // keyboard and the accessibility tree cannot disagree. Built as
  // offset + count per group in one pass: the render loop then
  // reads `group.start + offset` instead of searching the flat list
  // for each row.
  const { flat, rendered } = useMemo(() => {
    const ordered: Command[] = [];
    const layout = groups.map((group) => {
      const start = ordered.length;
      ordered.push(...group.commands);
      return { label: group.label, start, count: group.commands.length };
    });
    return { flat: ordered, rendered: layout };
  }, [groups]);
  // Clamp rather than trust `activeIndex`: the list can shrink
  // between the arrow key that set the index and the render that
  // consumes it (a keystroke filters the rows, and a late-arriving
  // session page replaces the loaded set), and an out-of-range index
  // would leave nothing highlighted with `Enter` doing nothing.
  const active = flat.length === 0 ? -1 : Math.min(activeIndex, flat.length - 1);

  const runCommand = useCallback(
    (command: Command | undefined) => {
      if (!command) return;
      onClose();
      command.run();
    },
    [onClose],
  );

  const isLoading = sessions.loading || agents.loading;

  // Keep the highlighted row inside the list's visible window when
  // the keyboard walks past the fold (a query can fill up to three
  // groups of twenty rows, far more than the 46vh list shows).
  // `block: 'nearest'` targets the list — the nearest scrollable
  // ancestor — and is a no-op for a row that is already visible, so
  // pointer movement between visible rows cannot cause a jump.
  useEffect(() => {
    if (active === -1) return;
    document.getElementById(`nt-palette-option-${active}`)?.scrollIntoView({ block: 'nearest' });
  }, [active]);

  /**
   * Arrow / Enter / Tab handling lives on the wrapper rather than on
   * the search field: focus can be on the close button (Tab moves
   * there), and the list must stay drivable from wherever focus sits
   * inside the dialog.
   */
  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Tab') {
      // Keep focus inside the dialog. `Modal` handles Escape, focus
      // capture and focus restore, but not tab wrapping — without
      // this the user can tab out of an `aria-modal` dialog into the
      // page behind it.
      const panel = event.currentTarget.closest('[role="dialog"]');
      if (!panel) return;
      const focusable = Array.from(
        panel.querySelectorAll<HTMLElement>(
          'input, button, [href], select, textarea, [tabindex]:not([tabindex="-1"])',
        ),
      );
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (!first || !last) return;
      const current = document.activeElement;
      const outside = !(current instanceof HTMLElement) || !panel.contains(current);
      if (event.shiftKey ? current === first || outside : current === last || outside) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
      }
      return;
    }

    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      if (flat.length === 0) return;
      event.preventDefault();
      const step = event.key === 'ArrowDown' ? 1 : -1;
      setActiveIndex((active + step + flat.length) % flat.length);
      return;
    }

    if (event.key === 'Home' || event.key === 'End') {
      if (flat.length === 0) return;
      event.preventDefault();
      setActiveIndex(event.key === 'Home' ? 0 : flat.length - 1);
      return;
    }

    // Only the search field submits. Without this guard, Enter on
    // the close button would fire the highlighted command as well as
    // activating the button.
    if (event.key === 'Enter' && event.target instanceof HTMLInputElement) {
      // An IME candidate window owns Enter while it is composing —
      // committing a Chinese/Japanese candidate must not also run a
      // command.
      if (event.nativeEvent.isComposing) return;
      event.preventDefault();
      runCommand(flat[active]);
    }
  };

  return (
    <div className="nt-palette__body" onKeyDown={handleKeyDown}>
      <Input
        className="nt-palette__search"
        type="text"
        role="combobox"
        aria-expanded="true"
        aria-controls="nt-palette-list"
        aria-activedescendant={active === -1 ? undefined : `nt-palette-option-${active}`}
        aria-label="Search commands and every resource"
        aria-autocomplete="list"
        autoFocus
        autoComplete="off"
        spellCheck={false}
        placeholder="Search commands, tools, skills, sessions…"
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setActiveIndex(0);
        }}
        data-testid="palette-input"
      />
      <div
        className="nt-palette__list"
        id="nt-palette-list"
        role="listbox"
        aria-label="Commands"
        data-testid="palette-list"
      >
        {flat.length === 0 ? (
          <p className="nt-palette__empty" data-testid="palette-empty">
            {isLoading ? 'Loading…' : `No matches for “${query.trim()}”.`}
          </p>
        ) : (
          rendered.map((group) => (
            <div key={group.label} role="group" aria-label={group.label}>
              <div className="nt-palette__group-label" aria-hidden>
                {group.label}
              </div>
              {flat.slice(group.start, group.start + group.count).map((command, offset) => {
                const index = group.start + offset;
                return (
                  <div
                    key={command.id}
                    id={`nt-palette-option-${index}`}
                    role="option"
                    aria-selected={index === active}
                    className="nt-palette__option"
                    data-testid={`palette-option-${command.id}`}
                    onMouseMove={() => setActiveIndex(index)}
                    onClick={() => runCommand(command)}
                  >
                    <span className="nt-palette__option-label">{command.label}</span>
                    {command.detail && (
                      <span className="nt-palette__option-detail">{command.detail}</span>
                    )}
                    {command.shortcut && (
                      <span className="nt-palette__option-shortcut" aria-hidden>
                        {command.shortcut}
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          ))
        )}
      </div>
      <p className="nt-palette__hint">
        <kbd>↑</kbd>
        <kbd>↓</kbd> move · <kbd>⏎</kbd> open · <kbd>esc</kbd> close
      </p>
    </div>
  );
}
