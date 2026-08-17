import { useEffect, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { Markdown } from '../components/chat/Markdown';
import { Heading } from '@radix-ui/themes';
import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { MetadataTable, type MetadataRow } from '../components/ui/MetadataTable';
import { emptyCell, pillCell, stringListCell } from '../components/ui/metadataCells';
import { api } from '../api/client';
import type { AgentDetail } from '../api/types';

/**
 * Read-only inspector for a single agent descriptor.
 *
 * The server-side contract is: a registered agent is a real
 * ReAct agent — the descriptor is what was used to instantiate
 * it. Editing it would change nothing at runtime because the
 * runtime copies come from `AppState` and from the registered
 * `Arc<dyn Agent>`. So we present this page as a viewer only;
 * mutation goes through `POST/DELETE /api/v1/agents`.
 *
 * Renders two surfaces:
 *   - the descriptor fields as a metadata table
 *   - the agent instructions (system prompt) as rendered
 *     markdown so structured prompts render as headings /
 *     bullet lists / tables
 */
export function AgentDetailPage() {
  const { name = '' } = useParams<{ name: string }>();
  const [detail, setDetail] = useState<AgentDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    // Forward an AbortController so the fetch is actually
    // cancelled when the component unmounts or `name` changes —
    // without this, a fast route change still leaves a dangling
    // socket and can surface an unhandled rejection.
    const controller = new AbortController();
    setDetail(null);
    setError(null);
    api
      .get<AgentDetail>(
        `/api/v1/agents/${encodeURIComponent(name)}`,
        controller.signal,
      )
      .then((d) => {
        if (!cancelled) setDetail(d);
      })
      .catch((e: Error) => {
        if (cancelled || e.name === 'AbortError') return;
        setError(e.message);
      });
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [name]);

  if (error) {
    return (
      <div>
        <Heading as="h1" size="6">
          Agent
        </Heading>
        <EmptyState
          icon="⚠️"
          title="Failed to load agent"
          description={<code style={{ color: 'var(--text-muted)' }}>{error}</code>}
          testId="agent-detail-error"
        />
        <Link to="/agents">
          <Button variant="soft">Back to Agents</Button>
        </Link>
      </div>
    );
  }

  if (!detail) {
    return (
      <div>
        <Heading as="h1" size="6">
          Agent
        </Heading>
        <SkeletonList count={2} testId="agent-detail-skeleton" />
      </div>
    );
  }

  const rows: MetadataRow[] = [
    { label: 'Name', value: detail.name },
    { label: 'Kind', value: detail.kind },
    { label: 'Version', value: detail.version },
    {
      label: 'Capabilities',
      value: stringListCell(detail.capabilities) ?? emptyCell(),
    },
    { label: 'Tools', value: stringListCell(detail.tools) ?? emptyCell() },
    {
      label: 'Handoffs',
      value: stringListCell(detail.handoffs) ?? emptyCell(),
    },
    { label: 'Owner', value: detail.owner ?? emptyCell() },
    { label: 'Domain', value: detail.domain ?? emptyCell() },
    { label: 'Persona', value: detail.persona ?? emptyCell() },
    { label: 'Model Hint', value: detail.modelHint ?? emptyCell() },
    {
      label: 'Source',
      value: pillCell(detail.protected ? 'built-in' : 'user-defined'),
    },
    { label: 'Description', value: detail.description },
  ];

  return (
    <div>
      <Heading as="h1" size="6">
        Agent: {detail.name}
        {detail.protected && (
          <span style={{ marginLeft: 12 }}>
            <code className="nt-pill" data-testid="agent-protected-pill">
              built-in
            </code>
          </span>
        )}
      </Heading>
      <Link to="/agents">
        <Button variant="soft" data-testid="agent-detail-back">
          Back
        </Button>
      </Link>

      <Card title="Metadata">
        <MetadataTable rows={rows} />
      </Card>

      <Card title="Instructions">
        {detail.instructions.trim().length === 0 ? (
          <span className="nt-pill nt-pill--muted">No instructions</span>
        ) : (
          <div className="nt-markdown" data-testid="agent-instructions-markdown">
            <Markdown source={detail.instructions} />
          </div>
        )}
      </Card>
    </div>
  );
}
