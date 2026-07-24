import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';
import { Input } from '../components/ui/Input';
import { useEffect, useState, type FormEvent } from 'react';
import { api } from '../api/client';
import type { McpServer } from '../api/types';

interface McpServersListResponse {
  servers: McpServer[];
  count: number;
}

export function McpPage() {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [name, setName] = useState('');
  const [command, setCommand] = useState('');
  const [args, setArgs] = useState('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .get<McpServersListResponse>('/api/mcp/servers')
      .then((data) => {
        if (cancelled) return;
        const list = (data.servers ?? []).map((s) => ({ ...s, id: s.id ?? s.name }));
        setServers(list);
      })
      .catch((e: Error) => {
        if (cancelled) return;
        setError(e.message);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const add = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !command.trim()) return;
    const argList = args
      .split(/\s+/)
      .map((a) => a.trim())
      .filter((a) => a.length > 0);
    try {
      const created = await api.post<McpServer>('/api/mcp/servers', {
        name,
        command,
        args: argList,
      });
      setServers((prev) => [...prev, { ...created, id: created.id ?? created.name }]);
      setName('');
      setCommand('');
      setArgs('');
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const remove = async (id: string) => {
    setServers((prev) => prev.filter((s) => s.id !== id));
    try {
      await api.del(`/api/mcp/servers/${encodeURIComponent(id)}`);
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
            label="Command"
            value={command}
            onChange={(e) => setCommand(e.target.value)}
            placeholder="e.g. npx"
            data-testid="mcp-command"
          />
          <Input
            label="Args"
            value={args}
            onChange={(e) => setArgs(e.target.value)}
            placeholder="space-separated args"
            data-testid="mcp-args"
          />
          <Button type="submit" data-testid="mcp-add">
            Add
          </Button>
        </form>
      </Card>
      {servers.length === 0 && !error && <Card title="No servers">No MCP servers registered.</Card>}
      {servers.map((server) => (
        <Card key={server.id} title={server.name} glow="green">
          <code>command: {server.command}</code>
          {server.args && server.args.length > 0 && (
            <div>
              <code>args: {server.args.join(' ')}</code>
            </div>
          )}
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
