import { useState, type FormEvent } from 'react';
import { Link } from 'react-router-dom';
import { Heading } from '@radix-ui/themes';
import { Button } from '../components/ui/Button';
import { Input } from '../components/ui/Input';
import { Modal } from '../components/ui/Modal';
import { useCursorList } from '../hooks/useCursorList';
import { useListFilter } from '../hooks/useListFilter';
import { ListToolbar } from '../components/ui/ListToolbar';
import { useToast } from '../hooks/useToast';
import { api } from '../api/client';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import type { AgentDetail, AgentDescriptorRequest } from '../api/types';

const KIND_OPTIONS = ['react', 'pipeline', 'router'];

interface FormState {
  name: string;
  description: string;
  kind: string;
  version: string;
  instructions: string;
  capabilities: string;
  tools: string;
  handoffs: string;
  owner: string;
  domain: string;
  persona: string;
}

const EMPTY_FORM: FormState = {
  name: '',
  description: '',
  kind: 'react',
  version: '0.1.0',
  instructions: '',
  capabilities: '',
  tools: '',
  handoffs: '',
  owner: '',
  domain: '',
  persona: '',
};

/**
 * Parse a comma-separated list, trimming whitespace and dropping
 * empty entries. Used for capabilities / tools / handoffs.
 */
function splitList(s: string): string[] {
  return s
    .split(',')
    .map((x) => x.trim())
    .filter(Boolean);
}

/**
 * Agent registry view. The page is list-first: the toolbar
 * exposes search + sort + a "Create Agent" button. The
 * registration form lives behind a modal dialog so the list
 * isn't pushed below a multi-field block.
 *
 * Each list row links to the agent's detail page; the inline
 * delete button lets the user remove a non-protected agent
 * without leaving the list. The server instantiates each
 * registered descriptor as a real ReAct agent, so what shows
 * here is what's actually running.
 */
export function AgentsPage() {
  const {
    items: agents,
    loading,
    error,
    setItems,
    refresh,
  } = useCursorList<AgentDetail>('/api/v1/agents');

  const {
    filtered: visibleAgents,
    query: agentsQuery,
    setQuery: setAgentsQuery,
    sortDir: agentsSortDir,
    setSortDir: setAgentsSortDir,
    isFiltering: isAgentsFiltering,
  } = useListFilter(agents, {
    match: (a, q) => a.name.toLowerCase().includes(q),
    compare: (a, b) => a.name.localeCompare(b.name),
  });

  const [createOpen, setCreateOpen] = useState(false);
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<Record<string, boolean>>({});
  const toast = useToast();

  const update =
    (k: keyof FormState) =>
    (v: string): void =>
      setForm((f) => ({ ...f, [k]: v }));

  const closeCreate = (): void => {
    setCreateOpen(false);
    setSubmitError(null);
    setForm(EMPTY_FORM);
  };

  const submit = async (e: FormEvent): Promise<void> => {
    e.preventDefault();
    setSubmitting(true);
    setSubmitError(null);
    try {
      const body: AgentDescriptorRequest = {
        name: form.name.trim(),
        description: form.description.trim(),
        kind: form.kind,
        version: form.version.trim() || '0.1.0',
        instructions: form.instructions,
        capabilities: splitList(form.capabilities),
        tools: splitList(form.tools),
        handoffs: splitList(form.handoffs),
        owner: form.owner.trim() || undefined,
        domain: form.domain.trim() || undefined,
        persona: form.persona.trim() || undefined,
      };
      const created = await api.post<AgentDetail>('/api/v1/agents', body);
      setItems((prev) => [...prev, created]);
      toast.push({
        variant: 'success',
        message: `Registered agent "${created.name}".`,
      });
      closeCreate();
    } catch (e) {
      setSubmitError((e as Error).message);
    } finally {
      setSubmitting(false);
    }
  };

  const deleteAgent = async (agent: AgentDetail): Promise<void> => {
    if (!window.confirm(`Delete agent '${agent.name}'?`)) return;
    setPendingDelete((p) => ({ ...p, [agent.name]: true }));
    try {
      await api.del(`/api/v1/agents/${encodeURIComponent(agent.name)}`);
      setItems((prev) => prev.filter((a) => a.name !== agent.name));
      toast.push({
        variant: 'success',
        message: `Deleted agent "${agent.name}".`,
      });
    } catch (e) {
      window.alert((e as Error).message);
    } finally {
      setPendingDelete((p) => ({ ...p, [agent.name]: false }));
    }
  };

  return (
    <div>
      <Heading as="h1" size="6">
        Agents
      </Heading>

      <ListToolbar
        query={agentsQuery}
        onQueryChange={setAgentsQuery}
        sortDir={agentsSortDir}
        onSortDirChange={setAgentsSortDir}
        searchLabel="Agents"
        testId="agents-toolbar"
      >
        <Button variant="soft" onClick={refresh} disabled={loading} data-testid="agents-refresh">
          {loading ? 'Refreshing...' : 'Refresh'}
        </Button>
        <Button variant="solid" onClick={() => setCreateOpen(true)} data-testid="agents-create">
          + Create Agent
        </Button>
      </ListToolbar>

      {error ? (
        <EmptyState
          icon="⚠️"
          title="Failed to load agents"
          description={error}
          testId="agents-error"
        />
      ) : loading && agents.length === 0 ? (
        <SkeletonList count={3} testId="agents-skeleton" />
      ) : visibleAgents.length === 0 && !error && !loading ? (
        <EmptyState
          icon={isAgentsFiltering ? '🔍' : '🤖'}
          title={isAgentsFiltering ? 'No agents match your search' : 'No agents registered'}
          description={
            isAgentsFiltering
              ? `No agents matched "${agentsQuery}". Try clearing the search.`
              : 'Click "Create Agent" to register one. The default React agent will pick up SKILL.md files from your workspace automatically.'
          }
          testId="agents-empty"
          action={
            !isAgentsFiltering ? (
              <Button
                variant="solid"
                onClick={() => setCreateOpen(true)}
                data-testid="agents-create-empty"
              >
                + Create Agent
              </Button>
            ) : undefined
          }
        />
      ) : null}

      {visibleAgents.length > 0 && (
        <ul className="nt-agent__list" data-testid="agents-list">
          {visibleAgents.map((a) => (
            <li key={a.name}>
              <Link
                to={`/agents/${encodeURIComponent(a.name)}`}
                className="nt-agent__row"
                data-testid={`agent-row-${a.name}`}
              >
                <div className="nt-agent__row-main">
                  <div className="nt-agent__row-name">
                    <span>{a.protected ? `${a.name} (built-in)` : a.name}</span>
                    <span className="nt-agent__row-meta">
                      {a.kind}
                      {a.protected ? ' · built-in' : ''}
                    </span>
                  </div>
                  {a.description && <p className="nt-agent__row-desc">{a.description}</p>}
                </div>
                <div
                  className="nt-agent__row-actions"
                  onClick={(e) => {
                    // Stop the row link from navigating when
                    // the user clicks the inline delete button —
                    // the link would otherwise swallow the event
                    // and the user would land on the detail
                    // page instead of seeing the confirm dialog.
                    e.preventDefault();
                    e.stopPropagation();
                  }}
                >
                  <Button
                    variant="soft"
                    color="red"
                    size="1"
                    onClick={() => deleteAgent(a)}
                    disabled={!!pendingDelete[a.name] || a.protected}
                    data-testid={`agent-delete-${a.name}`}
                  >
                    {pendingDelete[a.name] ? 'Deleting...' : 'Delete'}
                  </Button>
                </div>
              </Link>
            </li>
          ))}
        </ul>
      )}

      <Modal
        open={createOpen}
        onClose={closeCreate}
        title="Create Agent"
        testId="agent-create-modal"
        footer={
          <>
            <Button variant="soft" onClick={closeCreate} data-testid="agent-cancel">
              Cancel
            </Button>
            <Button
              variant="solid"
              onClick={(e) => {
                // The submit lives on the form below; synthesise
                // a submit click so the form's validation runs.
                const formEl = (e.currentTarget as HTMLButtonElement)
                  .closest('.nt-modal__panel')
                  ?.querySelector('form');
                if (formEl) {
                  if (typeof formEl.requestSubmit === 'function') {
                    formEl.requestSubmit();
                  } else {
                    formEl.dispatchEvent(new Event('submit', { cancelable: true, bubbles: true }));
                  }
                }
              }}
              disabled={submitting || !form.name.trim() || !form.description.trim()}
              loading={submitting}
              data-testid="agent-submit"
            >
              {submitting ? 'Registering...' : 'Register'}
            </Button>
          </>
        }
      >
        <form onSubmit={submit} className="nt-form" data-testid="agent-create-form">
          <Input
            label="Name"
            value={form.name}
            onChange={(e) => update('name')(e.target.value)}
            placeholder="my-agent"
            required
            data-testid="agent-name"
          />
          <Input
            label="Description"
            value={form.description}
            onChange={(e) => update('description')(e.target.value)}
            placeholder="Short description"
            required
            data-testid="agent-description"
          />
          <label className="nt-form__label">
            <span>Kind</span>
            <select
              value={form.kind}
              onChange={(e) => update('kind')(e.target.value)}
              data-testid="agent-kind"
            >
              {KIND_OPTIONS.map((k) => (
                <option key={k} value={k}>
                  {k}
                </option>
              ))}
            </select>
          </label>
          <Input
            label="Version"
            value={form.version}
            onChange={(e) => update('version')(e.target.value)}
            placeholder="0.1.0"
          />
          <label className="nt-form__label">
            <span>Instructions (system prompt)</span>
            <textarea
              rows={4}
              value={form.instructions}
              onChange={(e) => update('instructions')(e.target.value)}
              data-testid="agent-instructions"
            />
          </label>
          <Input
            label="Capabilities (comma-separated)"
            value={form.capabilities}
            onChange={(e) => update('capabilities')(e.target.value)}
            placeholder="tools, streaming"
          />
          <Input
            label="Tools (comma-separated)"
            value={form.tools}
            onChange={(e) => update('tools')(e.target.value)}
            placeholder="read_file, shell"
          />
          <Input
            label="Handoffs (comma-separated)"
            value={form.handoffs}
            onChange={(e) => update('handoffs')(e.target.value)}
            placeholder="explorer, code"
          />
          <Input
            label="Owner"
            value={form.owner}
            onChange={(e) => update('owner')(e.target.value)}
            placeholder="team-a"
          />
          <Input
            label="Domain"
            value={form.domain}
            onChange={(e) => update('domain')(e.target.value)}
            placeholder="coding"
          />
          <Input
            label="Persona"
            value={form.persona}
            onChange={(e) => update('persona')(e.target.value)}
            placeholder="helpful reviewer"
          />
          {submitError && (
            <p className="nt-form__error" role="alert" data-testid="agent-submit-error">
              <code>{submitError}</code>
            </p>
          )}
          {/* Hidden submit button so pressing Enter inside any
            input still triggers form validation + submit. The
            visible "Register" button is rendered in the modal
            footer above. */}
          <button type="submit" hidden tabIndex={-1} aria-hidden="true" />
        </form>
      </Modal>
    </div>
  );
}
