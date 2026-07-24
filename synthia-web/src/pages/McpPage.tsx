import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';
import { Input } from '../components/ui/Input';
import { useEffect, useState, type FormEvent } from 'react';

interface McpServer {
  id: string;
  name: string;
  url: string;
  status?: string;
}

export function McpPage() {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch('/api/mcp/servers')
      .then((r) => (r.ok ? r.json() : []))
      .then((data) => setServers(Array.isArray(data) ? data : (data.servers ?? [])))
      .catch((e) => setError(e.message));
  }, []);

  const add = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !url.trim()) return;
    try {
      const res = await fetch('/api/mcp/servers', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, url }),
      });
      if (res.ok) {
        const created = await res.json();
        setServers((prev) => [...prev, created]);
        setName('');
        setUrl('');
      } else {
        setError(`Add failed: HTTP ${res.status}`);
      }
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const remove = async (id: string) => {
    setServers((prev) => prev.filter((s) => s.id !== id));
    try {
      await fetch(`/api/mcp/servers/${encodeURIComponent(id)}`, {
        method: 'DELETE',
      });
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <div>
      <h1 className="nt-page-title">MCP Servers</h1>
      {error && (
        <Card glow="red" title="Error">
          <code>{error}</code>
        </Card>
      )}
      <Card title="Add Server" glow="cyan">
        <form onSubmit={add} className="nt-page-form">
          <Input
            label="Name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. github-mcp"
            data-testid="mcp-name"
          />
          <Input
            label="URL"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="http://localhost:3000"
            data-testid="mcp-url"
          />
          <Button type="submit" data-testid="mcp-add">
            Add
          </Button>
        </form>
      </Card>
      {servers.length === 0 && <Card title="No servers">No MCP servers registered.</Card>}
      {servers.map((server) => (
        <Card key={server.id} title={server.name} glow="green">
          <code>url: {server.url}</code>
          {server.status && (
            <div>
              <code>status: {server.status}</code>
            </div>
          )}
          <div>
            <Button
              variant="danger"
              onClick={() => remove(server.id)}
              data-testid={`mcp-remove-${server.id}`}
            >
              Remove
            </Button>
          </div>
        </Card>
      ))}
    </div>
  );
}
