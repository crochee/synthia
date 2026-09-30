import { useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';
import { Box } from '@radix-ui/themes';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { BackLink, PageHero, PageShell, Section, StatusPill } from '../components/ui/visual';
import { workflowsApi } from '../api/workflows';
import type { WorkflowRunResult, WorkflowSpec } from '../api/types';
import { useToast } from '../hooks/useToast';

const WORKFLOW_GLYPH = '🔀';
const WORKFLOW_HERO_MODIFIER = 'nt-vis-hero--tool';

/**
 * Workflow detail page.
 *
 * Live data: `GET /api/v1/workflows/{id}` returns the full
 * `WorkflowSpec`. The "Run" button posts to
 * `POST /api/v1/workflows/{id}/run`; today the backend
 * returns a stubbed `WorkflowRunResult` (every call settles
 * as `superseded`) until a real `WorkflowHost` is wired.
 */
export function WorkflowDetailPage() {
  const { id } = useParams<{ id: string }>();
  const [spec, setSpec] = useState<WorkflowSpec | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [runResult, setRunResult] = useState<WorkflowRunResult | null>(null);
  const [running, setRunning] = useState(false);
  const toast = useToast();

  useEffect(() => {
    if (!id) return;
    const controller = new AbortController();
    setLoading(true);
    setError(null);
    workflowsApi
      .get(id, controller.signal)
      .then((s) => setSpec(s))
      .catch((e: unknown) => {
        if ((e as { name?: string } | null)?.name === 'AbortError') return;
        setError((e as Error).message);
      })
      .finally(() => setLoading(false));
    return () => controller.abort();
  }, [id]);

  const handleRun = async (): Promise<void> => {
    if (!id) return;
    setRunning(true);
    try {
      const result = await workflowsApi.run(id);
      setRunResult(result);
      toast.push({
        variant: 'info',
        message: `Run ${result.run_id} finished (${result.calls.length} call(s)).`,
      });
    } catch (e) {
      toast.push({ variant: 'error', message: (e as Error).message });
    } finally {
      setRunning(false);
    }
  };

  return (
    <PageShell testId="workflow-detail-page">
      <BackLink to="/workflows" testId="workflow-detail-back">
        Back to workflows
      </BackLink>
      <PageHero
        glyph={WORKFLOW_GLYPH}
        heroModifier={WORKFLOW_HERO_MODIFIER}
        title={spec ? <code>{spec.id}</code> : id ? <code>{id}</code> : 'Workflow'}
        subtitle="Detail view for one workflow document. Run posts to /api/v1/workflows/{id}/run; today the runtime is stubbed (every call settles as `superseded`)."
        pills={spec ? <StatusPill tone="purple">WorkflowSpec</StatusPill> : undefined}
        meta={
          spec ? (
            <div className="nt-vis-meta-cluster">
              <div className="nt-vis-meta-cluster__row">
                <span>Steps</span>
                <span>{spec.steps.length}</span>
              </div>
              <div className="nt-vis-meta-cluster__row">
                <span>Phases</span>
                <span>{spec.phases?.length ?? 0}</span>
              </div>
            </div>
          ) : undefined
        }
        testId="workflow-detail-hero"
      />

      <Section
        title="Spec"
        glyph="📄"
        testId="workflow-detail-spec"
        meta={
          spec ? (
            <Button
              variant="solid"
              size="1"
              onClick={() => void handleRun()}
              disabled={running}
              loading={running}
              data-testid="workflow-detail-run-button"
            >
              {running ? 'Running…' : 'Run'}
            </Button>
          ) : undefined
        }
      >
        {error ? (
          <EmptyState
            icon="⚠️"
            title="Could not load workflow"
            description={error}
            testId="workflow-detail-error"
          />
        ) : loading ? (
          <SkeletonList count={2} testId="workflow-detail-skeleton" />
        ) : spec ? (
          <pre className="nt-vis-code" data-testid="workflow-detail-spec-code">
            <code>{JSON.stringify(spec, null, 2)}</code>
          </pre>
        ) : null}
      </Section>

      {runResult && (
        <Section title="Run" glyph="▶️" testId="workflow-detail-run">
          <Box style={{ display: 'grid', gap: 'var(--spacing-sm)' }}>
            <div>
              <strong>Run id:</strong> <code>{runResult.run_id}</code>
            </div>
            <div>
              <strong>Calls:</strong> {runResult.calls.length} (spawned {runResult.spawned},
              replayed {runResult.replayed})
            </div>
            {runResult.output && (
              <div>
                <strong>Output:</strong>
                <pre className="nt-vis-code">
                  <code>{runResult.output}</code>
                </pre>
              </div>
            )}
          </Box>
          <Box mt="3">
            {runResult.calls.map((c) => (
              <Box
                key={c.position}
                data-testid={`workflow-run-call-${c.position}`}
                style={{
                  borderTop: '1px solid var(--border-subtle)',
                  padding: 'var(--spacing-sm) 0',
                }}
              >
                <StatusPill
                  tone={
                    c.status === 'succeeded' ? 'green' : c.status === 'failed' ? 'red' : 'yellow'
                  }
                >
                  {c.status}
                </StatusPill>{' '}
                <code>{c.step_id}</code>
                {c.error && (
                  <Box mt="1" style={{ color: 'var(--text-muted)' }}>
                    {c.error}
                  </Box>
                )}
              </Box>
            ))}
          </Box>
        </Section>
      )}
    </PageShell>
  );
}
