import { Card } from '../components/ui/Card';
import { Input } from '../components/ui/Input';
import { Button } from '../components/ui/Button';
import { useState, type FormEvent } from 'react';

interface MemoryHit {
  id: string;
  content: string;
  relevance?: number;
}

export function MemoryPage() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<MemoryHit[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const search = async (e?: FormEvent) => {
    e?.preventDefault();
    const q = query.trim();
    if (!q) return;
    setLoading(true);
    setError(null);
    try {
      const res = await fetch(`/api/memory/search?q=${encodeURIComponent(q)}`);
      const data = await res.json();
      setResults(Array.isArray(data) ? data : (data.results ?? []));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div>
      <h1 className="nt-page-title">Memory</h1>
      <form onSubmit={search} className="nt-page-form">
        <Input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search memory..."
          data-testid="memory-query"
        />
        <Button type="submit" disabled={loading} data-testid="memory-search">
          {loading ? 'Searching...' : 'Search'}
        </Button>
      </form>
      {error && (
        <Card glow="red" title="Error">
          <code>{error}</code>
        </Card>
      )}
      {results.length === 0 && !error && !loading && (
        <Card title="No results">No matching memories found.</Card>
      )}
      {results.map((hit) => (
        <Card key={hit.id} title={`Memory ${hit.id.slice(0, 8)}`} glow="cyan">
          <p>{hit.content}</p>
          {hit.relevance !== undefined && (
            <code>relevance: {(hit.relevance * 100).toFixed(1)}%</code>
          )}
        </Card>
      ))}
    </div>
  );
}
