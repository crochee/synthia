import { Card } from '../components/ui/Card';
import { useEffect, useState } from 'react';
import { api } from '../api/client';
import type { Tool } from '../api/types';

interface ToolListResponse {
  tools: Tool[];
  count: number;
}

export function ToolsPage() {
  const [tools, setTools] = useState<Tool[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .get<ToolListResponse>('/api/tools')
      .then((data) => {
        if (cancelled) return;
        setTools(data.tools ?? []);
      })
      .catch((e: Error) => {
        if (cancelled) return;
        setError(e.message);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div>
      <h1 className="nt-page-title">Tools</h1>
      {error && (
        <Card glow="red" title="Error">
          <code>{error}</code>
        </Card>
      )}
      {tools.length === 0 && !error && (
        <Card title="No tools">No tools are currently registered.</Card>
      )}
      {tools.map((tool) => (
        <Card key={tool.name} title={tool.name} glow="green">
          {tool.description && <p>{tool.description}</p>}
          {tool.status && <code>status: {tool.status}</code>}
        </Card>
      ))}
    </div>
  );
}
