import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { Box, Flex } from '@radix-ui/themes';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { ListToolbar } from '../components/ui/ListToolbar';
import { PageHero, PageShell, StatusPill, CardGrid } from '../components/ui/visual';
import { evalsApi } from '../api/evals';
import type { EvalSuite } from '../api/types';
import { useToast } from '../hooks/useToast';

const EVAL_GLYPH = '📊';
const EVAL_HERO_MODIFIER = 'nt-vis-hero--tool';
const EVAL_CARD_MODIFIER = 'nt-vis-card--tool';

/**
 * Evals list page.
 *
 * Live data: `GET /api/v1/evals` returns `EvalSuite[]`. Click a
 * card to see the cases at `/evals/:name`. The "+ New Suite"
 * modal posts a JSON suite to `/api/v1/evals`.
 */
export function EvalsPage() {
  const [suites, setSuites] = useState<EvalSuite[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [createName, setCreateName] = useState('');
  const [createSpec, setCreateSpec] = useState(
    JSON.stringify(
      {
        name: 'new-suite',
        cases: [
          {
            id: 'case-1',
            input: 'hello',
            expected_keywords: ['hello'],
          },
        ],
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
      const result = await evalsApi.list(signal);
      setSuites(result.data);
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
    let suite: EvalSuite;
    try {
      suite = JSON.parse(createSpec) as EvalSuite;
      if (!suite.name) throw new Error('suite.name is required');
      if (createName && createName !== suite.name) {
        throw new Error('suite.name does not match the name field above');
      }
    } catch (e) {
      setCreateError(`Invalid JSON: ${(e as Error).message}`);
      return;
    }
    setCreating(true);
    setCreateError(null);
    try {
      await evalsApi.create({ suite });
      toast.push({ variant: 'success', message: `Registered suite "${suite.name}".` });
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
      <StatusPill tone="cyan">{`${suites.length} suites`}</StatusPill>
      <StatusPill tone="yellow">offline grading</StatusPill>
    </>
  );

  return (
    <PageShell testId="evals-page">
      <PageHero
        glyph={EVAL_GLYPH}
        heroModifier={EVAL_HERO_MODIFIER}
        title="Evals"
        subtitle="Offline grading framework. Each EvalSuite holds TestCases; an EvalRunner scores every case against every configured metric (keyword / llm_judge / schema_validation). Reports export as JSON or CSV."
        pills={heroPills}
        meta={
          <div className="nt-vis-meta-cluster">
            <div className="nt-vis-meta-cluster__row">
              <span>Source</span>
              <span>synthia-eval · EvalRunner</span>
            </div>
            <div className="nt-vis-meta-cluster__row">
              <span>Threshold</span>
              <span>0.7 (default)</span>
            </div>
          </div>
        }
        testId="evals-hero"
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
        searchLabel="Evals"
        testId="evals-toolbar"
      >
        <Button
          variant="soft"
          onClick={() => void refresh()}
          disabled={loading}
          data-testid="evals-refresh"
        >
          {loading ? 'Refreshing…' : 'Refresh'}
        </Button>
        <Button variant="solid" onClick={() => setCreateOpen(true)} data-testid="evals-new">
          + New Suite
        </Button>
      </ListToolbar>

      {error ? (
        <EmptyState
          icon="⚠️"
          title="Failed to load suites"
          description={error}
          testId="evals-error"
        />
      ) : loading && suites.length === 0 ? (
        <SkeletonList count={3} testId="evals-skeleton" />
      ) : suites.length === 0 ? (
        <EmptyState
          icon="📊"
          title="No eval suites yet"
          description="Register a suite of TestCases (id / input / expected_keywords / expected_output) to surface it here. The runner today scores against the keyword metric only."
          testId="evals-empty"
        />
      ) : (
        <CardGrid testId="evals-grid">
          {suites.map((suite) => (
            <Link
              key={suite.name}
              to={`/evals/${encodeURIComponent(suite.name)}`}
              className={`nt-vis-card ${EVAL_CARD_MODIFIER}`}
              data-testid={`eval-card-${suite.name}`}
            >
              <Flex justify="between" align="start" mb="2">
                <Box>
                  <h3 className="nt-vis-card__name" title={suite.name}>
                    <code>{suite.name}</code>
                  </h3>
                  <span className="nt-vis-card__subtitle">
                    {suite.cases.length} case{suite.cases.length === 1 ? '' : 's'}
                  </span>
                </Box>
                <StatusPill tone="yellow">EvalSuite</StatusPill>
              </Flex>
              <div className="nt-vis-card__footer">
                <div className="nt-vis-card__pills">
                  <StatusPill tone="cyan">offline</StatusPill>
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
          aria-label="Create eval suite"
          data-testid="evals-create-modal"
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
            data-testid="evals-create-form"
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
            <h2 style={{ marginTop: 0 }}>New eval suite</h2>
            <label className="nt-form__label">
              <span>Suite name</span>
              <input
                type="text"
                value={createName}
                onChange={(e) => {
                  const v = e.target.value;
                  setCreateName(v);
                  try {
                    const parsed = JSON.parse(createSpec) as Record<string, unknown>;
                    parsed.name = v;
                    setCreateSpec(JSON.stringify(parsed, null, 2));
                  } catch {
                    /* ignore */
                  }
                }}
                placeholder="new-suite"
                required
                data-testid="evals-create-name"
              />
            </label>
            <label className="nt-form__label">
              <span>EvalSuite (JSON)</span>
              <textarea
                rows={16}
                value={createSpec}
                onChange={(e) => setCreateSpec(e.target.value)}
                spellCheck={false}
                data-testid="evals-create-spec"
              />
            </label>
            <p className="nt-form__hint">
              Each case has id, input, expected_keywords, and optional expected_output. The runner
              today only uses the keyword metric; LLM-judge and schema-validation land with a real
              EvalAgent.
            </p>
            {createError && (
              <p className="nt-form__error" role="alert" data-testid="evals-create-error">
                <code>{createError}</code>
              </p>
            )}
            <Flex gap="2" justify="end" mt="3">
              <Button
                variant="soft"
                type="button"
                onClick={() => setCreateOpen(false)}
                data-testid="evals-create-cancel"
              >
                Cancel
              </Button>
              <Button
                variant="solid"
                type="submit"
                disabled={creating || !createName.trim()}
                loading={creating}
                data-testid="evals-create-submit"
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
