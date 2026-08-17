import { useEffect, useState } from 'react';
import { useParams, Link } from 'react-router-dom';
import { Heading } from '@radix-ui/themes';
import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { api } from '../api/client';
import type { ToolDetail } from '../api/types';

/**
 * Read-only inspector for a single tool. Shows the description,
 * the input JSON Schema (which the LLM uses for tool_choice),
 * and the tool's provenance — `core` for tools compiled into
 * the binary, `dynamic` for tools registered at runtime.
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
      .get<ToolDetail>(
        `/api/v1/tools/${encodeURIComponent(name)}`,
        controller.signal,
      )
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

  if (error) {
    return (
      <div>
        <Heading as="h1" size="6">
          Tool
        </Heading>
        <EmptyState
          icon="⚠️"
          title="Failed to load tool"
          description={<code style={{ color: 'var(--text-muted)' }}>{error}</code>}
          testId="tool-detail-error"
        />
        <Link to="/tools">
          <Button variant="soft">Back</Button>
        </Link>
      </div>
    );
  }

  if (!tool) {
    return (
      <div>
        <Heading as="h1" size="6">
          Tool
        </Heading>
        <SkeletonList count={2} testId="tool-detail-skeleton" />
      </div>
    );
  }

  const schemaText =
    tool.input_schema && Object.keys(tool.input_schema).length > 0
      ? JSON.stringify(tool.input_schema, null, 2)
      : '(no schema)';

  return (
    <div>
      <Heading as="h1" size="6">
        Tool: {tool.name}
      </Heading>
      <Link to="/tools">
        <Button variant="soft" data-testid="tool-detail-back">
          Back
        </Button>
      </Link>

      <Card title="Metadata">
        <code>provenance: {tool.provenance}</code>
        {tool.description && <p>{tool.description}</p>}
      </Card>

      <Card title="Input Schema">
        <pre>
          <code>{schemaText}</code>
        </pre>
      </Card>
    </div>
  );
}
