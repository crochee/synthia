import { Link } from 'react-router-dom';
import { Heading } from '@radix-ui/themes';
import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { ListToolbar } from '../components/ui/ListToolbar';
import { useCursorList } from '../hooks/useCursorList';
import { useListFilter } from '../hooks/useListFilter';
import type { Tool } from '../api/types';

export function ToolsPage() {
  const { items: tools, loading, error, hasMore, loadMore } = useCursorList<Tool>('/api/v1/tools');

  const {
    filtered: visibleTools,
    query,
    setQuery,
    sortDir,
    setSortDir,
    isFiltering,
  } = useListFilter(tools, {
    match: (t, q) => {
      // Match by name + description. Description can be
      // undefined for tool records that don't carry one.
      if (t.name.toLowerCase().includes(q)) return true;
      if (t.description && t.description.toLowerCase().includes(q)) return true;
      return false;
    },
    compare: (a, b) => a.name.localeCompare(b.name),
  });

  return (
    <div>
      <Heading as="h1" size="6">
        Tools
      </Heading>
      <ListToolbar
        query={query}
        onQueryChange={setQuery}
        sortDir={sortDir}
        onSortDirChange={setSortDir}
        searchLabel="Tools"
        testId="tools-toolbar"
      />
      {error ? (
        <EmptyState
          icon="⚠️"
          title="Failed to load tools"
          description={error}
          testId="tools-error"
        />
      ) : loading && tools.length === 0 ? (
        <SkeletonList count={4} testId="tools-skeleton" />
      ) : visibleTools.length === 0 && !error ? (
        <EmptyState
          icon={isFiltering ? '🔍' : '🧰'}
          title={isFiltering ? 'No tools match your search' : 'No tools registered'}
          description={
            isFiltering
              ? `No tools matched "${query}". Try clearing the search.`
              : 'Tools become available here once an agent descriptor references them.'
          }
          testId="tools-empty"
        />
      ) : (
        visibleTools.map((tool) => (
          <Card
            key={tool.name}
            title={
              <Link
                to={`/tools/${encodeURIComponent(tool.name)}`}
                data-testid={`tool-link-${tool.name}`}
              >
                {tool.name}
              </Link>
            }
          >
            {tool.description && <p>{tool.description}</p>}
          </Card>
        ))
      )}
      {hasMore && (
        <div>
          <Button
            variant="soft"
            onClick={loadMore}
            disabled={loading}
            data-testid="tools-load-more"
          >
            {loading ? 'Loading...' : 'Load More'}
          </Button>
        </div>
      )}
    </div>
  );
}
