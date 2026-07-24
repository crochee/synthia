import { Card } from '../components/ui/Card';
import { useEffect, useState } from 'react';

interface Tool {
  name: string;
  description?: string;
  status?: string;
}

export function ToolsPage() {
  const [tools, setTools] = useState<Tool[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch('/api/tools')
      .then((r) => (r.ok ? r.json() : []))
      .then((data) => setTools(Array.isArray(data) ? data : (data.tools ?? [])))
      .catch((e) => setError(e.message));
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
