import { useEffect, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { Markdown } from '../components/chat/Markdown';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { MetadataTable, type MetadataRow } from '../components/ui/MetadataTable';
import { emptyCell, pillCell, stringListCell } from '../components/ui/metadataCells';
import {
  PageHero,
  PageShell,
  Property,
  Section,
  StatusPill,
  BackLink,
  agentTheme,
  statusTone,
} from '../components/ui/visual';
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
 * Renders three surfaces:
 *   - the page hero (icon, name, kind, source pills)
 *   - a property grid surfacing the descriptor fields at a
 *     glance (kind / version / model hint / persona / source)
 *   - a metadata table for the full set of descriptor fields
 *     so power users still have a single-glance reference
 *   - a section card holding the agent instructions (system
 *     prompt) as rendered markdown so structured prompts
 *     render as headings / bullet lists / tables
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
      .get<AgentDetail>(`/api/v1/agents/${encodeURIComponent(name)}`, controller.signal)
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
      <PageShell testId="agent-detail-page">
        <BackLink to="/agents">Back to Agents</BackLink>
        <EmptyState
          icon="⚠️"
          title="Failed to load agent"
          description={<code style={{ color: 'var(--text-muted)' }}>{error}</code>}
          testId="agent-detail-error"
        />
        <div>
          <Link to="/agents">
            <Button variant="soft">Back to Agents</Button>
          </Link>
        </div>
      </PageShell>
    );
  }

  if (!detail) {
    return (
      <PageShell testId="agent-detail-page">
        <BackLink to="/agents">Back to Agents</BackLink>
        <SkeletonList count={2} testId="agent-detail-skeleton" />
      </PageShell>
    );
  }

  const theme = agentTheme(detail.protected);

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

  const heroPills = (
    <>
      <StatusPill tone={statusTone(detail.kind)}>{detail.kind}</StatusPill>
      <StatusPill tone={detail.protected ? 'yellow' : 'green'}>
        {detail.protected ? 'built-in' : 'user-defined'}
      </StatusPill>
      <StatusPill tone="ghost">v{detail.version}</StatusPill>
      {detail.modelHint && <StatusPill tone="cyan">{detail.modelHint}</StatusPill>}
    </>
  );

  return (
    <PageShell testId="agent-detail-page">
      <BackLink to="/agents" testId="agent-detail-back">
        Back to Agents
      </BackLink>

      <PageHero
        glyph={theme.glyph}
        heroModifier={theme.heroModifier}
        title={
          <>
            <code>{detail.name}</code>
            {detail.protected && (
              <StatusPill tone="yellow" testId="agent-protected-pill">
                built-in
              </StatusPill>
            )}
          </>
        }
        subtitle={detail.description || 'No description provided.'}
        pills={heroPills}
        meta={
          <div className="nt-vis-meta-cluster">
            <div className="nt-vis-meta-cluster__row">
              <span>Kind</span>
              <span>{detail.kind}</span>
            </div>
            <div className="nt-vis-meta-cluster__row">
              <span>Version</span>
              <span>v{detail.version}</span>
            </div>
          </div>
        }
        testId="agent-hero"
      />

      <Section title="Overview" glyph="📋" testId="agent-overview">
        <div className="nt-vis-prop-grid">
          <Property
            label="Kind"
            value={detail.kind}
            accent={statusTone(detail.kind)}
            testId="agent-prop-kind"
          />
          <Property
            label="Version"
            value={`v${detail.version}`}
            accent="blue"
            testId="agent-prop-version"
          />
          <Property
            label="Source"
            value={detail.protected ? 'built-in' : 'user-defined'}
            accent={detail.protected ? 'yellow' : 'green'}
            testId="agent-prop-source"
          />
          <Property
            label="Model"
            value={detail.modelHint ?? emptyCell()}
            accent={detail.modelHint ? 'cyan' : undefined}
            testId="agent-prop-model"
          />
          <Property
            label="Persona"
            value={detail.persona ?? emptyCell()}
            accent={detail.persona ? 'purple' : undefined}
            testId="agent-prop-persona"
          />
          <Property
            label="Domain"
            value={detail.domain ?? emptyCell()}
            accent={detail.domain ? 'blue' : undefined}
            testId="agent-prop-domain"
          />
          <Property label="Owner" value={detail.owner ?? emptyCell()} testId="agent-prop-owner" />
        </div>
      </Section>

      {(detail.capabilities.length > 0 ||
        detail.tools.length > 0 ||
        detail.handoffs.length > 0) && (
        <Section title="Capabilities" glyph="🧩" testId="agent-capabilities">
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--spacing-md)' }}>
            {detail.capabilities.length > 0 && (
              <div>
                <div
                  style={{
                    fontFamily: 'var(--font-mono)',
                    fontSize: 'var(--fs-xs)',
                    textTransform: 'uppercase',
                    letterSpacing: '0.06em',
                    color: 'var(--text-muted)',
                    marginBottom: 'var(--spacing-xs)',
                  }}
                >
                  Capabilities
                </div>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px' }}>
                  {detail.capabilities.map((c) => (
                    <StatusPill key={c} tone="blue">
                      {c}
                    </StatusPill>
                  ))}
                </div>
              </div>
            )}
            {detail.tools.length > 0 && (
              <div>
                <div
                  style={{
                    fontFamily: 'var(--font-mono)',
                    fontSize: 'var(--fs-xs)',
                    textTransform: 'uppercase',
                    letterSpacing: '0.06em',
                    color: 'var(--text-muted)',
                    marginBottom: 'var(--spacing-xs)',
                  }}
                >
                  Tools
                </div>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px' }}>
                  {detail.tools.map((t) => (
                    <StatusPill key={t} tone="cyan">
                      {t}
                    </StatusPill>
                  ))}
                </div>
              </div>
            )}
            {detail.handoffs.length > 0 && (
              <div>
                <div
                  style={{
                    fontFamily: 'var(--font-mono)',
                    fontSize: 'var(--fs-xs)',
                    textTransform: 'uppercase',
                    letterSpacing: '0.06em',
                    color: 'var(--text-muted)',
                    marginBottom: 'var(--spacing-xs)',
                  }}
                >
                  Handoffs
                </div>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px' }}>
                  {detail.handoffs.map((h) => (
                    <StatusPill key={h} tone="purple">
                      {h}
                    </StatusPill>
                  ))}
                </div>
              </div>
            )}
          </div>
        </Section>
      )}

      <Section title="Full descriptor" glyph="🗂️" testId="agent-full-descriptor">
        <MetadataTable rows={rows} />
      </Section>

      <Section title="Instructions" glyph="📜" testId="agent-instructions">
        {detail.instructions.trim().length === 0 ? (
          <EmptyState
            icon="📭"
            title="No instructions"
            description="This agent runs without a custom system prompt — the model uses its base behaviour."
            testId="agent-instructions-empty"
          />
        ) : (
          <div
            className="nt-markdown"
            data-testid="agent-instructions-markdown"
            style={{
              padding: 'var(--spacing-md)',
              background:
                'linear-gradient(135deg, rgba(124, 58, 237, 0.04) 0%, rgba(37, 99, 235, 0.04) 100%)',
              border: '1px solid var(--border-subtle)',
              borderRadius: 'var(--radius-md)',
              borderLeft: '3px solid var(--accent-purple)',
            }}
          >
            <Markdown source={detail.instructions} />
          </div>
        )}
      </Section>
    </PageShell>
  );
}
