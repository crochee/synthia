import { useEffect, useState, useMemo } from 'react';
import { useParams, Link } from 'react-router-dom';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import {
  ENTITY_THEMES,
  PageHero,
  PageShell,
  Property,
  Section,
  StatusPill,
  BackLink,
  statusTone,
} from '../components/ui/visual';
import { api } from '../api/client';
import type { ToolDetail } from '../api/types';

const TOOL_THEME = ENTITY_THEMES.tool;

/**
 * Quick visual summary of a JSON Schema's structure.
 *
 * The LLM-facing tool schema is a recursive object — listing
 * every key inline would explode the property grid. Instead
 * we summarise the top-level shape: how many properties the
 * `properties` block declares, whether `required` is set,
 * and whether there are nested objects.
 */
interface SchemaSummary {
  propertyCount: number;
  requiredCount: number;
  topLevelKeys: ReadonlyArray<string>;
  hasNestedObjects: boolean;
}

function summarizeSchema(schema: Record<string, unknown> | null | undefined): SchemaSummary {
  const result: SchemaSummary = {
    propertyCount: 0,
    requiredCount: 0,
    topLevelKeys: Object.keys(schema ?? {}),
    hasNestedObjects: false,
  };
  if (!schema || typeof schema !== 'object') return result;
  const props = (schema as { properties?: Record<string, unknown> }).properties;
  if (props && typeof props === 'object') {
    result.propertyCount = Object.keys(props).length;
    for (const v of Object.values(props)) {
      if (v && typeof v === 'object' && (v as { type?: string }).type === 'object') {
        result.hasNestedObjects = true;
        break;
      }
    }
  }
  const required = (schema as { required?: unknown }).required;
  if (Array.isArray(required)) {
    result.requiredCount = required.length;
  }
  return result;
}

/**
 * Read-only inspector for a single tool. Shows the description,
 * a summary of the input JSON Schema (which the LLM uses for
 * tool_choice), the raw schema in a syntax-tinted block, and
 * the tool's provenance — `core` for tools compiled into the
 * binary, `dynamic` for tools registered at runtime.
 */
export function ToolDetailPage() {
  const { name = '' } = useParams<{ name: string }>();
  const [tool, setTool] = useState<ToolDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    // Forward an AbortController so the fetch is actually
    // cancelled on unmount or `name` change — without it, a
    // fast route change leaves a dangling socket and can
    // surface an unhandled rejection.
    const controller = new AbortController();
    setTool(null);
    setError(null);
    api
      .get<ToolDetail>(`/api/v1/tools/${encodeURIComponent(name)}`, controller.signal)
      .then((t) => {
        if (!cancelled) setTool(t);
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

  const schemaSummary = useMemo(() => summarizeSchema(tool?.input_schema), [tool?.input_schema]);

  if (error) {
    return (
      <PageShell testId="tool-detail-page">
        <BackLink to="/tools">Back to Tools</BackLink>
        <EmptyState
          icon="⚠️"
          title="Failed to load tool"
          description={<code style={{ color: 'var(--text-muted)' }}>{error}</code>}
          testId="tool-detail-error"
        />
        <div>
          <Link to="/tools">
            <Button variant="soft">Back to Tools</Button>
          </Link>
        </div>
      </PageShell>
    );
  }

  if (!tool) {
    return (
      <PageShell testId="tool-detail-page">
        <BackLink to="/tools">Back to Tools</BackLink>
        <SkeletonList count={2} testId="tool-detail-skeleton" />
      </PageShell>
    );
  }

  const schemaText =
    tool.input_schema && Object.keys(tool.input_schema).length > 0
      ? JSON.stringify(tool.input_schema, null, 2)
      : '(no schema)';

  const heroPills = (
    <>
      <StatusPill tone={statusTone(tool.provenance)}>{tool.provenance}</StatusPill>
      <StatusPill tone="cyan">{`${schemaSummary.propertyCount} properties`}</StatusPill>
      {schemaSummary.requiredCount > 0 && (
        <StatusPill tone="yellow">{`${schemaSummary.requiredCount} required`}</StatusPill>
      )}
    </>
  );

  return (
    <PageShell testId="tool-detail-page">
      <BackLink to="/tools" testId="tool-detail-back">
        Back to Tools
      </BackLink>

      <PageHero
        glyph={TOOL_THEME.glyph}
        heroModifier={TOOL_THEME.heroModifier}
        title={<code>{tool.name}</code>}
        subtitle={tool.description || 'No description provided.'}
        pills={heroPills}
        meta={
          <div className="nt-vis-meta-cluster">
            <div className="nt-vis-meta-cluster__row">
              <span>Type</span>
              <span>function-call</span>
            </div>
          </div>
        }
        testId="tool-hero"
      />

      <Section title="Overview" glyph="📋" testId="tool-overview">
        <div className="nt-vis-prop-grid">
          <Property
            label="Name"
            value={<code>{tool.name}</code>}
            accent="cyan"
            testId="tool-prop-name"
          />
          <Property
            label="Provenance"
            value={tool.provenance}
            accent={tool.provenance === 'core' ? 'blue' : 'purple'}
            testId="tool-prop-provenance"
          />
          <Property
            label="Schema fields"
            value={`${schemaSummary.propertyCount}`}
            accent="green"
            testId="tool-prop-schema-fields"
          />
          <Property
            label="Required fields"
            value={schemaSummary.requiredCount > 0 ? `${schemaSummary.requiredCount}` : '—'}
            accent={schemaSummary.requiredCount > 0 ? 'yellow' : undefined}
            testId="tool-prop-required"
          />
        </div>
      </Section>

      <Section title="Description" glyph="💬" testId="tool-description">
        <p style={{ margin: 0, color: 'var(--text-primary)', lineHeight: 'var(--lh-normal)' }}>
          {tool.description || (
            <span style={{ color: 'var(--text-muted)' }}>No description provided.</span>
          )}
        </p>
      </Section>

      <Section
        title="Input Schema"
        glyph="📐"
        meta={
          schemaSummary.hasNestedObjects ? (
            <StatusPill tone="purple">nested objects</StatusPill>
          ) : null
        }
        testId="tool-schema"
      >
        <pre className="nt-vis-code" data-testid="tool-schema-code">
          <code>{schemaText}</code>
        </pre>
      </Section>
    </PageShell>
  );
}
