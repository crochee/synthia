import { useEffect, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { Markdown } from '../components/chat/Markdown';
import { Heading } from '@radix-ui/themes';
import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { MetadataTable, type MetadataRow } from '../components/ui/MetadataTable';
import { emptyCell } from '../components/ui/metadataCells';
import { api } from '../api/client';
import type { SkillDetail } from '../api/types';

/**
 * Read-only inspector for a single skill.
 *
 * Renders the SKILL.md as two stacked surfaces:
 *   - the parsed frontmatter as a metadata table
 *   - the markdown body as rendered HTML
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
      <div>
        <Heading as="h1" size="6">
          Skill
        </Heading>
        <EmptyState
          icon="⚠️"
          title="Failed to load skill"
          description={<code style={{ color: 'var(--text-muted)' }}>{error}</code>}
          testId="skill-detail-error"
        />
        <Link to="/skills">
          <Button variant="soft">Back to Skills</Button>
        </Link>
      </div>
    );
  }

  if (!detail) {
    return (
      <div>
        <Heading as="h1" size="6">
          Skill
        </Heading>
        <SkeletonList count={2} testId="skill-detail-skeleton" />
      </div>
    );
  }

  // Build metadata rows from the parsed frontmatter. We render
  // every recognised key explicitly so the order is stable and
  // documented in the type signature, and fall back to the raw
  // frontmatter map for any extra keys.
  const fm = detail.frontmatter;
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
    <div>
      <Heading as="h1" size="6">
        Skill: {detail.name}
      </Heading>
      <Link to="/skills">
        <Button variant="soft" data-testid="skill-detail-back">
          Back
        </Button>
      </Link>

      <Card title="Metadata">
        <MetadataTable rows={frontmatterRows} />
      </Card>

      <Card title="Body">
        {detail.body.trim().length === 0 ? (
          <span className="nt-pill nt-pill--muted">No markdown body</span>
        ) : (
          <div className="nt-markdown" data-testid="skill-markdown-body">
            <Markdown source={detail.body} />
          </div>
        )}
      </Card>
    </div>
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
