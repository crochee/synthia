import { useEffect, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { Markdown } from '../components/chat/Markdown';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { MetadataTable, type MetadataRow } from '../components/ui/MetadataTable';
import { emptyCell } from '../components/ui/metadataCells';
import {
  ENTITY_THEMES,
  PageHero,
  PageShell,
  Property,
  Section,
  StatusPill,
  BackLink,
} from '../components/ui/visual';
import { api } from '../api/client';
import type { SkillDetail } from '../api/types';

const SKILL_THEME = ENTITY_THEMES.skill;

/**
 * Read-only inspector for a single skill.
 *
 * Renders the SKILL.md as three stacked surfaces:
 *   - the page hero (icon, name, description, path pill)
 *   - a property grid of the most-looked-at frontmatter
 *     fields (name / description / path) so a glanceable
 *     summary is available without scrolling
 *   - a section card holding the rendered markdown body
 *   - a metadata table for any remaining frontmatter keys
 *     so power users still have a flat "everything" view
 */
export function SkillDetailPage() {
  const { name = '' } = useParams<{ name: string }>();
  const [detail, setDetail] = useState<SkillDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    // Forward an AbortController so the fetch is actually
    // cancelled when the component unmounts (or `name` flips to
    // a different skill before the response lands). Without
    // this, a fast route change still leaves a dangling socket
    // and can surface an unhandled rejection in the console.
    const controller = new AbortController();
    setDetail(null);
    setError(null);
    api
      .get<SkillDetail>(`/api/v1/skills/${encodeURIComponent(name)}`, controller.signal)
      .then((d) => {
        if (!cancelled) setDetail(d);
      })
      .catch((e: Error) => {
        // AbortError is the expected path on unmount — skip the
        // setError so the UI doesn't flash "Failed to load" for
        // a request the user already abandoned.
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
      <PageShell testId="skill-detail-page">
        <BackLink to="/skills">Back to Skills</BackLink>
        <EmptyState
          icon="⚠️"
          title="Failed to load skill"
          description={<code style={{ color: 'var(--text-muted)' }}>{error}</code>}
          testId="skill-detail-error"
        />
        <div>
          <Link to="/skills">
            <Button variant="soft">Back to Skills</Button>
          </Link>
        </div>
      </PageShell>
    );
  }

  if (!detail) {
    return (
      <PageShell testId="skill-detail-page">
        <BackLink to="/skills">Back to Skills</BackLink>
        <SkeletonList count={2} testId="skill-detail-skeleton" />
      </PageShell>
    );
  }

  // Build metadata rows from the parsed frontmatter. We render
  // every recognised key explicitly so the order is stable and
  // documented in the type signature, and fall back to the raw
  // frontmatter map for any extra keys.
  const fm = detail.frontmatter;
  const heroPills = (
    <>
      <StatusPill tone="purple">SKILL.md</StatusPill>
      {detail.description && <StatusPill tone="green">registered</StatusPill>}
    </>
  );

  const frontmatterRows: MetadataRow[] = [
    { label: 'Name', value: (fm['name'] as string | undefined) ?? detail.name },
    {
      label: 'Description',
      value: (fm['description'] as string | undefined) ?? detail.description,
    },
    {
      label: 'Path',
      value: <code>{detail.path}</code>,
    },
  ];
  const metadata = fm['metadata'];
  if (
    metadata !== null &&
    metadata !== undefined &&
    typeof metadata === 'object' &&
    !Array.isArray(metadata)
  ) {
    for (const [k, v] of Object.entries(metadata as Record<string, unknown>)) {
      frontmatterRows.push({ label: k, value: formatMetadataValue(v) });
    }
  }

  return (
    <PageShell testId="skill-detail-page">
      <BackLink to="/skills" testId="skill-detail-back">
        Back to Skills
      </BackLink>

      <PageHero
        glyph={SKILL_THEME.glyph}
        heroModifier={SKILL_THEME.heroModifier}
        title={<code>{detail.name}</code>}
        subtitle={detail.description || 'No description provided.'}
        pills={heroPills}
        meta={
          <div className="nt-vis-meta-cluster">
            <div className="nt-vis-meta-cluster__row">
              <span>Path</span>
              <span
                title={detail.path}
                style={{
                  maxWidth: 280,
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                }}
              >
                {detail.path}
              </span>
            </div>
          </div>
        }
        testId="skill-hero"
      />

      <Section title="Overview" glyph="📋" testId="skill-overview">
        <div className="nt-vis-prop-grid">
          <Property
            label="Name"
            value={<code>{detail.name}</code>}
            accent="purple"
            testId="skill-prop-name"
          />
          <Property
            label="Description"
            value={detail.description || emptyCell()}
            accent="blue"
            testId="skill-prop-description"
          />
          <Property
            label="Source"
            value={<code style={{ fontSize: '0.85em' }}>{detail.path}</code>}
            accent="cyan"
            testId="skill-prop-path"
          />
        </div>
      </Section>

      {frontmatterRows.length > 0 && (
        <Section title="Frontmatter" glyph="🗂️" testId="skill-frontmatter">
          <MetadataTable rows={frontmatterRows} />
        </Section>
      )}

      <Section title="Body" glyph="📝" testId="skill-body">
        {detail.body.trim().length === 0 ? (
          <EmptyState
            icon="📭"
            title="No markdown body"
            description="This SKILL.md only declares frontmatter — the agent has no extra instructions to follow."
            testId="skill-body-empty"
          />
        ) : (
          <div className="nt-markdown" data-testid="skill-markdown-body">
            <Markdown source={detail.body} />
          </div>
        )}
      </Section>
    </PageShell>
  );
}

/**
 * Format an arbitrary frontmatter value as React content.
 * Strings/numbers/booleans render inline; objects/arrays fall
 * back to a JSON dump so the data is still visible to the user.
 */
function formatMetadataValue(v: unknown): React.ReactNode {
  if (v === null || v === undefined) return emptyCell();
  if (typeof v === 'string') return v;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  return <code>{JSON.stringify(v)}</code>;
}
