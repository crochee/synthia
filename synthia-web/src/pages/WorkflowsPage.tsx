import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { Box, Flex } from '@radix-ui/themes';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { ListToolbar } from '../components/ui/ListToolbar';
import { PageHero, PageShell, StatusPill, CardGrid } from '../components/ui/visual';
import { workflowsApi, type WorkflowSummary } from '../api/workflows';
import type { WorkflowSpec } from '../api/types';
import { useToast } from '../hooks/useToast';

const WORKFLOW_GLYPH = '🔀';
const WORKFLOW_HERO_MODIFIER = 'nt-vis-hero--tool';
const WORKFLOW_CARD_MODIFIER = 'nt-vis-card--tool';

export type { WorkflowSummary };

/**
 * Workflows list page.
 *
 * Live data: `GET /api/v1/workflows` returns `WorkflowSummary[]`
 * (id / step_count / phase_count). Click a card to see the
 * full document at `/workflows/:id`.
 *
 * The "+ New Workflow" modal scaffolds a minimal `WorkflowSpec`
 * with one agent step and posts to `/api/v1/workflows`. The
 * structured step wizard (5 variants × recursive) lands in a
 * follow-up turn; for now the modal exposes the JSON editor so
 * power users can ship arbitrary specs.
 */
export function WorkflowsPage() {
  const [workflows, setWorkflows] = useState<WorkflowSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [createId, setCreateId] = useState('');
  const [createSpec, setCreateSpec] = useState(
    JSON.stringify(
      {
        id: 'new-workflow',
        steps: [{ kind: 'agent', id: 'step1', agent: 'echo', prompt: 'say hi' }],
        phases: [],
      },
      null,
      2,
    ),
  );
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const toast = useToast();

  const refresh = async (signal?: AbortSignal): Promise<void> => {
    setLoading(true);
    setError(null);
    try {
      const result = await workflowsApi.list(signal);
      setWorkflows(result.data);
    } catch (e) {
      if ((e as { name?: string } | null)?.name === 'AbortError') return;
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    const controller = new AbortController();
    void refresh(controller.signal);
    return () => controller.abort();
  }, []);

  const submitCreate = async (): Promise<void> => {
    let spec: WorkflowSpec;
    try {
      spec = JSON.parse(createSpec) as WorkflowSpec;
      if (!spec.id) {
        throw new Error('spec.id is required');
      }
      if (createId && createId !== spec.id) {
        throw new Error('spec.id does not match the id field above');
      }
    } catch (e) {
      setCreateError(`Invalid JSON: ${(e as Error).message}`);
      return;
    }
    setCreating(true);
    setCreateError(null);
    try {
      await workflowsApi.create({ spec });
      toast.push({ variant: 'success', message: `Registered workflow "${spec.id}".` });
      setCreateOpen(false);
      await refresh();
    } catch (e) {
      setCreateError((e as Error).message);
    } finally {
      setCreating(false);
    }
  };

  const heroPills = (
    <>
      <StatusPill tone="cyan">{`${workflows.length} documents`}</StatusPill>
    </>
  );

  return (
    <PageShell testId="workflows-page">
      <PageHero
        glyph={WORKFLOW_GLYPH}
        heroModifier={WORKFLOW_HERO_MODIFIER}
        title="Workflows"
        subtitle="Multi-agent orchestration documents. Each WorkflowSpec is JSON: ordered steps (agent / fan_out / best_of / pipeline / mcts), optional phase labels. The runtime is synchronous from the caller's perspective — one POST returns the whole WorkflowRun."
        pills={heroPills}
        meta={
          <div className="nt-vis-meta-cluster">
            <div className="nt-vis-meta-cluster__row">
              <span>Source</span>
              <span>synthia-workflow · WorkflowRuntime</span>
            </div>
            <div className="nt-vis-meta-cluster__row">
              <span>Shape</span>
              <span>WorkflowSpec (JSON)</span>
            </div>
          </div>
        }
        testId="workflows-hero"
      />

      <ListToolbar
        query=""
        onQueryChange={() => {
          /* client-side filter placeholder; list is fetch-once */
        }}
        sortDir="asc"
        onSortDirChange={() => {
          /* sort placeholder */
        }}
        searchLabel="Workflows"
        testId="workflows-toolbar"
      >
        <Button
          variant="soft"
          onClick={() => void refresh()}
          disabled={loading}
          data-testid="workflows-refresh"
        >
          {loading ? 'Refreshing…' : 'Refresh'}
        </Button>
        <Button variant="solid" onClick={() => setCreateOpen(true)} data-testid="workflows-new">
          + New Workflow
        </Button>
      </ListToolbar>

      {error ? (
        <EmptyState
          icon="⚠️"
          title="Failed to load workflows"
          description={error}
          testId="workflows-error"
        />
      ) : loading && workflows.length === 0 ? (
        <SkeletonList count={3} testId="workflows-skeleton" />
      ) : workflows.length === 0 ? (
        <EmptyState
          icon="🔀"
          title="No workflows yet"
          description="Create a WorkflowSpec with the modal — one agent step by default. The structured step wizard (5 variants × recursive) lands in a follow-up turn."
          testId="workflows-empty"
        />
      ) : (
        <CardGrid testId="workflows-grid">
          {workflows.map((w) => (
            <Link
              key={w.id}
              to={`/workflows/${encodeURIComponent(w.id)}`}
              className={`nt-vis-card ${WORKFLOW_CARD_MODIFIER}`}
              data-testid={`workflow-card-${w.id}`}
            >
              <Flex justify="between" align="start" mb="2">
                <Box>
                  <h3 className="nt-vis-card__name" title={w.id}>
                    <code>{w.id}</code>
                  </h3>
                  <span className="nt-vis-card__subtitle">
                    {w.step_count} step{w.step_count === 1 ? '' : 's'}
                    {w.phase_count > 0 &&
                      ` · ${w.phase_count} phase${w.phase_count === 1 ? '' : 's'}`}
                  </span>
                </Box>
                <StatusPill tone="purple">WorkflowSpec</StatusPill>
              </Flex>
              <div className="nt-vis-card__footer">
                <div className="nt-vis-card__pills">
                  <StatusPill tone="cyan">document</StatusPill>
                </div>
              </div>
            </Link>
          ))}
        </CardGrid>
      )}

      {createOpen && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label="Create workflow"
          data-testid="workflows-create-modal"
          style={{
            position: 'fixed',
            inset: 0,
            background: 'rgba(0,0,0,0.5)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 1000,
          }}
        >
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void submitCreate();
            }}
            data-testid="workflows-create-form"
            className="nt-form"
            style={{
              background: 'var(--bg-primary)',
              padding: 'var(--spacing-lg)',
              borderRadius: 'var(--radius-md)',
              minWidth: 640,
              maxWidth: 900,
              maxHeight: '90vh',
              overflow: 'auto',
            }}
          >
            <h2 style={{ marginTop: 0 }}>New workflow</h2>
            <label className="nt-form__label">
              <span>Workflow id</span>
              <input
                type="text"
                value={createId}
                onChange={(e) => {
                  const v = e.target.value;
                  setCreateId(v);
                  try {
                    const parsed = JSON.parse(createSpec) as Record<string, unknown>;
                    parsed.id = v;
                    setCreateSpec(JSON.stringify(parsed, null, 2));
                  } catch {
                    /* ignore — user is typing */
                  }
                }}
                placeholder="new-workflow"
                required
                data-testid="workflows-create-id"
              />
            </label>
            <label className="nt-form__label">
              <span>WorkflowSpec (JSON)</span>
              <textarea
                rows={20}
                value={createSpec}
                onChange={(e) => setCreateSpec(e.target.value)}
                spellCheck={false}
                data-testid="workflows-create-spec"
              />
            </label>
            <p className="nt-form__hint">
              The backend plans the document with default caps before accepting. Cap-violations
              surface here as a 4xx. The structured step picker (Agent / FanOut / BestOf / Pipeline
              / MCTS) lands in a follow-up turn.
            </p>
            {createError && (
              <p className="nt-form__error" role="alert" data-testid="workflows-create-error">
                <code>{createError}</code>
              </p>
            )}
            <Flex gap="2" justify="end" mt="3">
              <Button
                variant="soft"
                type="button"
                onClick={() => setCreateOpen(false)}
                data-testid="workflows-create-cancel"
              >
                Cancel
              </Button>
              <Button
                variant="solid"
                type="submit"
                disabled={creating || !createId.trim()}
                loading={creating}
                data-testid="workflows-create-submit"
              >
                {creating ? 'Creating…' : 'Create'}
              </Button>
            </Flex>
          </form>
        </div>
      )}
    </PageShell>
  );
}
