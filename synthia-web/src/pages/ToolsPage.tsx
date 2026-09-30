import { useEffect, useState, type FormEvent } from 'react';
import { Link } from 'react-router-dom';
import { Box, Flex } from '@radix-ui/themes';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { Input } from '../components/ui/Input';
import { Modal } from '../components/ui/Modal';
import { SkeletonList } from '../components/ui/SkeletonList';
import { ListToolbar } from '../components/ui/ListToolbar';
import {
  ENTITY_THEMES,
  PageHero,
  PageShell,
  Section,
  StatusPill,
  CardGrid,
} from '../components/ui/visual';
import { api } from '../api/client';
import { useToast } from '../hooks/useToast';
import { useCursorList } from '../hooks/useCursorList';
import { useListFilter } from '../hooks/useListFilter';
import { tasksApi } from '../api/tasks';
import type { RegisterToolRequest, Tool, TaskDelegation } from '../api/types';

const TOOL_THEME = ENTITY_THEMES.tool;

/** Default JSON Schema shown in the Register Tool modal — a
 *  single free-text argument. The server registers a passthrough
 *  tool that echoes its arguments back, so the schema is the
 *  whole contract the model sees. */
const DEFAULT_SCHEMA = JSON.stringify(
  {
    type: 'object',
    properties: {
      input: { type: 'string', description: 'Text the tool echoes back.' },
    },
    required: ['input'],
  },
  null,
  2,
);

interface RegisterFormState {
  name: string;
  description: string;
  schema: string;
}

const EMPTY_REGISTER_FORM: RegisterFormState = {
  name: '',
  description: '',
  schema: DEFAULT_SCHEMA,
};

/**
 * Tool registry. Renders the same colour-themed card grid as
 * the other list pages so a user navigating between
 * Tools / Skills / Agents / Sessions gets a consistent
 * visual rhythm.
 *
 * The dynamic half of the lifecycle is wired here: register a
 * passthrough tool (name + description + JSON Schema) and
 * unregister one. Registrations live in the in-memory registry
 * and survive until the server restarts.
 */
export function ToolsPage() {
  const {
    items: tools,
    loading,
    error,
    hasMore,
    loadMore,
    refresh,
  } = useCursorList<Tool>('/api/v1/tools');

  const {
    filtered: visibleTools,
    query,
    setQuery,
    sortDir,
    setSortDir,
    isFiltering,
  } = useListFilter(tools, {
    match: (t, q) => {
      if (t.name.toLowerCase().includes(q)) return true;
      if (t.description && t.description.toLowerCase().includes(q)) return true;
      return false;
    },
    compare: (a, b) => a.name.localeCompare(b.name),
  });

  const [registerOpen, setRegisterOpen] = useState(false);
  const [registerForm, setRegisterForm] = useState<RegisterFormState>(EMPTY_REGISTER_FORM);
  const [registering, setRegistering] = useState(false);
  const [registerError, setRegisterError] = useState<string | null>(null);
  /** The tool awaiting unregister confirmation — a styled dialog
   *  instead of `window.confirm`, matching the other pages. */
  const [confirmTarget, setConfirmTarget] = useState<Tool | null>(null);
  const [pendingDelete, setPendingDelete] = useState<Record<string, boolean>>({});
  const toast = useToast();

  const total = tools.length;

  const closeRegister = (): void => {
    setRegisterOpen(false);
    setRegisterError(null);
    setRegisterForm(EMPTY_REGISTER_FORM);
  };

  const submitRegister = async (): Promise<void> => {
    // Parse the schema text first so a JSON typo is reported in
    // the form, not as a 500 from the server.
    let schema: Record<string, unknown>;
    try {
      const parsed: unknown = JSON.parse(registerForm.schema);
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        throw new Error('schema must be a JSON object');
      }
      schema = parsed as Record<string, unknown>;
    } catch (e) {
      setRegisterError(`Invalid JSON schema: ${(e as Error).message}`);
      return;
    }
    setRegistering(true);
    setRegisterError(null);
    try {
      const body: RegisterToolRequest = {
        name: registerForm.name.trim(),
        description: registerForm.description.trim(),
        input_schema: schema,
      };
      await api.post<Tool>('/api/v1/tools', body);
      toast.push({
        variant: 'success',
        message: `Registered tool "${body.name}".`,
      });
      closeRegister();
      await refresh();
    } catch (e) {
      setRegisterError((e as Error).message);
    } finally {
      setRegistering(false);
    }
  };

  const unregisterTool = async (tool: Tool): Promise<void> => {
    setConfirmTarget(null);
    setPendingDelete((p) => ({ ...p, [tool.name]: true }));
    try {
      await api.del(`/api/v1/tools/${encodeURIComponent(tool.name)}`);
      toast.push({
        variant: 'success',
        message: `Unregistered tool "${tool.name}".`,
      });
      await refresh();
    } catch (e) {
      toast.push({ variant: 'error', message: (e as Error).message });
    } finally {
      setPendingDelete((p) => ({ ...p, [tool.name]: false }));
    }
  };

  const heroPills = (
    <>
      <StatusPill tone="cyan" testId="tools-total-pill">
        {`${total} available`}
      </StatusPill>
      {isFiltering && (
        <StatusPill tone="yellow" withDot>
          filtering · {visibleTools.length}/{total}
        </StatusPill>
      )}
    </>
  );

  return (
    <PageShell testId="tools-page">
      <PageHero
        glyph={TOOL_THEME.glyph}
        heroModifier={TOOL_THEME.heroModifier}
        title="Tools"
        subtitle="Capabilities the agent can call mid-run. Register a dynamic passthrough tool here, or unregister one — registrations live until the server restarts."
        pills={heroPills}
        meta={
          <div className="nt-vis-meta-cluster">
            <div className="nt-vis-meta-cluster__row">
              <span>Source</span>
              <span>core / dynamic registry</span>
            </div>
            <div className="nt-vis-meta-cluster__row">
              <span>API</span>
              <span>/api/v1/tools</span>
            </div>
          </div>
        }
        testId="tools-hero"
      />

      <ListToolbar
        query={query}
        onQueryChange={setQuery}
        sortDir={sortDir}
        onSortDirChange={setSortDir}
        searchLabel="Tools"
        testId="tools-toolbar"
      >
        <Button
          variant="soft"
          onClick={() => void refresh()}
          disabled={loading}
          data-testid="tools-refresh"
        >
          {loading ? 'Refreshing...' : 'Refresh'}
        </Button>
        <Button variant="solid" onClick={() => setRegisterOpen(true)} data-testid="tools-register">
          + Register Tool
        </Button>
      </ListToolbar>

      {error ? (
        <EmptyState
          icon="⚠️"
          title="Failed to load tools"
          description={error}
          testId="tools-error"
        />
      ) : loading && tools.length === 0 ? (
        <SkeletonList count={6} testId="tools-skeleton" />
      ) : visibleTools.length === 0 && !error ? (
        <EmptyState
          icon={isFiltering ? '🔍' : '🛠️'}
          title={isFiltering ? 'No tools match your search' : 'No tools registered'}
          description={
            isFiltering
              ? `No tools matched "${query}". Try clearing the search.`
              : 'Register a dynamic tool here, or reference one from an agent descriptor.'
          }
          testId="tools-empty"
          action={
            !isFiltering ? (
              <Button
                variant="solid"
                onClick={() => setRegisterOpen(true)}
                data-testid="tools-register-empty"
              >
                + Register Tool
              </Button>
            ) : undefined
          }
        />
      ) : (
        <CardGrid testId="tools-grid">
          {visibleTools.map((tool) => {
            const href = `/tools/${encodeURIComponent(tool.name)}`;
            // The list endpoint (`GET /api/v1/tools`) returns the
            // minimal `Tool` shape (name + description only). The
            // provenance discriminator (`core` / `dynamic`) lives
            // on the detail endpoint, so we don't surface it as a
            // pill on the list card — the detail page surfaces it
            // instead. Surfacing it here would force every list
            // call to also fetch each tool's detail, which would
            // dominate the network for no clear user benefit.
            return (
              <Link
                key={tool.name}
                to={href}
                className={`nt-vis-card ${TOOL_THEME.cardModifier}`}
                data-testid={`tool-card-${tool.name}`}
              >
                <div className="nt-vis-card__header">
                  <div className="nt-vis-card__icon" aria-hidden>
                    {TOOL_THEME.glyph}
                  </div>
                  <div className="nt-vis-card__main">
                    <h3 className="nt-vis-card__name" title={tool.name}>
                      <code>{tool.name}</code>
                    </h3>
                    <span className="nt-vis-card__subtitle">Capability</span>
                  </div>
                </div>
                {tool.description ? (
                  <p className="nt-vis-card__desc">{tool.description}</p>
                ) : (
                  <p className="nt-vis-card__desc" style={{ color: 'var(--text-muted)' }}>
                    No description provided.
                  </p>
                )}
                <div className="nt-vis-card__footer">
                  <div className="nt-vis-card__pills">
                    <StatusPill tone="cyan">tool</StatusPill>
                  </div>
                  <div
                    className="nt-vis-card__actions"
                    onClick={(e) => {
                      // Keep the unregister click inside the card —
                      // without this the wrapping link swallows it
                      // and the user lands on the detail page.
                      e.preventDefault();
                      e.stopPropagation();
                    }}
                  >
                    <Button
                      variant="soft"
                      color="red"
                      size="1"
                      onClick={() => setConfirmTarget(tool)}
                      disabled={!!pendingDelete[tool.name]}
                      data-testid={`tool-unregister-${tool.name}`}
                    >
                      {pendingDelete[tool.name] ? 'Removing...' : 'Unregister'}
                    </Button>
                  </div>
                </div>
              </Link>
            );
          })}
        </CardGrid>
      )}

      {hasMore && (
        <div style={{ display: 'flex', justifyContent: 'center' }}>
          <Button
            variant="soft"
            onClick={loadMore}
            disabled={loading}
            data-testid="tools-load-more"
          >
            {loading ? 'Loading...' : 'Load More Tools'}
          </Button>
        </div>
      )}

      <RecentTaskDelegations />

      <Modal
        open={registerOpen}
        onClose={closeRegister}
        title="Register tool"
        testId="tool-register-modal"
        footer={
          <>
            <Button variant="soft" onClick={closeRegister} data-testid="tool-cancel">
              Cancel
            </Button>
            <Button
              variant="solid"
              onClick={() => void submitRegister()}
              disabled={
                registering || !registerForm.name.trim() || !registerForm.description.trim()
              }
              loading={registering}
              data-testid="tool-submit"
            >
              {registering ? 'Registering...' : 'Register'}
            </Button>
          </>
        }
      >
        <form
          onSubmit={(e: FormEvent) => {
            e.preventDefault();
            void submitRegister();
          }}
          className="nt-form"
          data-testid="tool-register-form"
        >
          <Input
            label="Name"
            value={registerForm.name}
            onChange={(e) => setRegisterForm((f) => ({ ...f, name: e.target.value }))}
            placeholder="echo"
            required
            data-testid="tool-name"
          />
          <Input
            label="Description"
            value={registerForm.description}
            onChange={(e) => setRegisterForm((f) => ({ ...f, description: e.target.value }))}
            placeholder="Echoes its arguments back."
            required
            data-testid="tool-description"
          />
          <label className="nt-form__label">
            <span>Input schema (JSON)</span>
            <textarea
              rows={10}
              value={registerForm.schema}
              onChange={(e) => setRegisterForm((f) => ({ ...f, schema: e.target.value }))}
              spellCheck={false}
              data-testid="tool-schema"
            />
          </label>
          <p className="nt-form__hint">
            The server registers a passthrough tool that echoes its arguments back — the schema is
            the whole contract the model sees.
          </p>
          {registerError && (
            <p className="nt-form__error" role="alert" data-testid="tool-submit-error">
              <code>{registerError}</code>
            </p>
          )}
          <button type="submit" hidden tabIndex={-1} aria-hidden="true" />
        </form>
      </Modal>

      <Modal
        open={confirmTarget !== null}
        onClose={() => setConfirmTarget(null)}
        title="Unregister tool"
        testId="tool-unregister-modal"
        footer={
          <>
            <Button
              variant="soft"
              onClick={() => setConfirmTarget(null)}
              data-testid="tool-unregister-cancel"
            >
              Cancel
            </Button>
            <Button
              variant="solid"
              color="red"
              onClick={() => confirmTarget && void unregisterTool(confirmTarget)}
              data-testid="tool-unregister-confirm"
            >
              Unregister
            </Button>
          </>
        }
      >
        <p>
          Unregister tool <strong>{confirmTarget?.name}</strong>? Agents stop seeing it immediately;
          the registration is gone until re-registered (server restart clears all dynamic tools
          anyway).
        </p>
      </Modal>
    </PageShell>
  );
}

/**
 * Recent sub-agent delegations surfaced on the Tools page.
 *
 * The `task` tool (`synthia-tool-task`) is the multi-agent
 * subagent delegation seam; its history is reconstructed from
 * session events at `/api/v1/sessions/{id}` (see
 * `src/api/tasks.ts`). Showing the 5 most recent entries here
 * gives the tools page a "what the agents are actually doing"
 * glance without forcing the user to navigate to `/tasks` first.
 *
 * The reconstruction fans out one GET per session (bounded by
 * `?limit=50` on the outer session query). When a dedicated
 * `/api/v1/tasks` endpoint ships, this collapses to a single
 * list call.
 */
function RecentTaskDelegations() {
  const [delegations, setDelegations] = useState<TaskDelegation[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    tasksApi
      .listAll(controller.signal)
      .then((result) => setDelegations(result.data.slice(0, 5)))
      .catch(() => {
        // Swallow — the section is decorative on the tools page;
        // a failure here should not replace the user's main
        // tools list with an error state.
      })
      .finally(() => setLoading(false));
    return () => controller.abort();
  }, []);

  return (
    <Section
      title="Recent task delegations"
      glyph="🧭"
      meta={
        <Link to="/tasks" style={{ textDecoration: 'none' }}>
          <StatusPill tone="cyan">view all</StatusPill>
        </Link>
      }
      testId="tools-recent-tasks"
    >
      {loading ? (
        <SkeletonList count={3} testId="tools-recent-tasks-skeleton" />
      ) : delegations.length === 0 ? (
        <EmptyState
          icon="🧭"
          title="No sub-agent delegations yet"
          description={
            <>
              Run a chat session that invokes the <code>task</code> tool to see its sub-agent
              envelopes appear here.
            </>
          }
          testId="tools-recent-tasks-empty"
        />
      ) : (
        <ul style={{ listStyle: 'none', padding: 0, margin: 0 }}>
          {delegations.map((task) => {
            const tone =
              task.status === 'succeeded'
                ? 'green'
                : task.status === 'failed' || task.status === 'aborted'
                  ? 'red'
                  : 'yellow';
            return (
              <li
                key={task.id}
                style={{
                  padding: 'var(--spacing-sm) 0',
                  borderBottom: '1px solid var(--border-subtle)',
                }}
              >
                <Flex align="center" justify="between" gap="2">
                  <Box>
                    <code>{task.agent}</code>
                    <span style={{ color: 'var(--text-muted)', marginLeft: 8 }}>
                      depth {task.parent_depth} · {task.parent_session_id.slice(0, 8)}
                    </span>
                  </Box>
                  <StatusPill tone={tone} withDot>
                    {task.status}
                  </StatusPill>
                </Flex>
                {task.prompt && (
                  <p
                    style={{
                      color: 'var(--text-secondary)',
                      fontSize: 'var(--fs-sm)',
                      margin: '4px 0 0',
                    }}
                  >
                    {task.prompt.length > 200 ? `${task.prompt.slice(0, 200)}…` : task.prompt}
                  </p>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </Section>
  );
}
