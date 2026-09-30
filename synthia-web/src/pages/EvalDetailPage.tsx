import { useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';
import { Box } from '@radix-ui/themes';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { BackLink, PageHero, PageShell, Section, StatusPill } from '../components/ui/visual';
import { evalsApi } from '../api/evals';
import type { EvalReport, EvalSuite, EvalTestResult } from '../api/types';
import { useToast } from '../hooks/useToast';

const EVAL_GLYPH = '📊';
const EVAL_HERO_MODIFIER = 'nt-vis-hero--tool';

/**
 * Eval detail page.
 *
 * Live data: `GET /api/v1/evals/{name}` returns the full
 * `EvalSuite`. The "Run" button posts to
 * `POST /api/v1/evals/{name}/run`; today the backend runs the
 * keyword metric only and returns the report.
 */
export function EvalDetailPage() {
  const { name } = useParams<{ name: string }>();
  const [suite, setSuite] = useState<EvalSuite | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<EvalReport | null>(null);
  const [running, setRunning] = useState(false);
  const toast = useToast();

  useEffect(() => {
    if (!name) return;
    const controller = new AbortController();
    setLoading(true);
    setError(null);
    evalsApi
      .get(name, controller.signal)
      .then((s) => setSuite(s))
      .catch((e: unknown) => {
        if ((e as { name?: string } | null)?.name === 'AbortError') return;
        setError((e as Error).message);
      })
      .finally(() => setLoading(false));
    return () => controller.abort();
  }, [name]);

  const handleRun = async (): Promise<void> => {
    if (!name) return;
    setRunning(true);
    try {
      const result = await evalsApi.run(name);
      setReport(result);
      toast.push({
        variant: 'info',
        message: `Run finished: ${result.passed}/${result.total} passed.`,
      });
    } catch (e) {
      toast.push({ variant: 'error', message: (e as Error).message });
    } finally {
      setRunning(false);
    }
  };

  return (
    <PageShell testId="eval-detail-page">
      <BackLink to="/evals" testId="eval-detail-back">
        Back to evals
      </BackLink>
      <PageHero
        glyph={EVAL_GLYPH}
        heroModifier={EVAL_HERO_MODIFIER}
        title={suite ? <code>{suite.name}</code> : name ? <code>{name}</code> : 'Eval suite'}
        subtitle="Detail view for one eval suite."
        pills={suite ? <StatusPill tone="yellow">EvalSuite</StatusPill> : undefined}
        testId="eval-detail-hero"
      />

      {error ? (
        <EmptyState
          icon="⚠️"
          title="Could not load suite"
          description={error}
          testId="eval-detail-error"
        />
      ) : loading ? (
        <SkeletonList count={2} testId="eval-detail-skeleton" />
      ) : suite ? (
        <>
          <Section
            title={`Cases (${suite.cases.length})`}
            glyph="📋"
            testId="eval-detail-cases"
            meta={
              <Button
                variant="solid"
                size="1"
                onClick={() => void handleRun()}
                disabled={running}
                loading={running}
                data-testid="eval-detail-run-button"
              >
                {running ? 'Running…' : 'Run'}
              </Button>
            }
          >
            <Box style={{ display: 'grid', gap: 'var(--spacing-sm)' }}>
              {suite.cases.map((c) => (
                <Box key={c.id} data-testid={`eval-case-${c.id}`}>
                  <code>{c.id}</code>: {c.input}
                  {c.expected_keywords && c.expected_keywords.length > 0 && (
                    <Box mt="1" style={{ color: 'var(--text-muted)' }}>
                      expects: {c.expected_keywords.join(', ')}
                    </Box>
                  )}
                </Box>
              ))}
            </Box>
          </Section>

          {report && <EvalReportView report={report} />}
        </>
      ) : null}
    </PageShell>
  );
}

function EvalReportView({ report }: { report: EvalReport }) {
  return (
    <Section title="Last Run" glyph="▶️" testId="eval-detail-run">
      <Box style={{ display: 'grid', gap: 'var(--spacing-sm)' }}>
        <div>
          <strong>Total:</strong> {report.total} · <strong>Passed:</strong> {report.passed} ·{' '}
          <strong>Failed:</strong> {report.total - report.passed}
        </div>
        <div>
          <strong>Average score:</strong> {report.average_score.toFixed(2)}
        </div>
      </Box>
      <Box mt="3">
        {report.results.map((r) => (
          <EvalResultRow key={r.case_id} result={r} />
        ))}
      </Box>
    </Section>
  );
}

function EvalResultRow({ result }: { result: EvalTestResult }) {
  return (
    <Box
      data-testid={`eval-result-${result.case_id}`}
      style={{ borderTop: '1px solid var(--border-subtle)', padding: 'var(--spacing-sm) 0' }}
    >
      <StatusPill tone={result.passed ? 'green' : 'red'}>
        {result.passed ? 'pass' : 'fail'}
      </StatusPill>{' '}
      <code>{result.case_id}</code>
      <Box mt="1" style={{ color: 'var(--text-muted)' }}>
        output: {result.actual_output.slice(0, 80)}
        {result.actual_output.length > 80 ? '…' : ''}
      </Box>
    </Box>
  );
}
