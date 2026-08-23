import { Link } from 'react-router-dom';
import { Markdown } from '../components/chat/Markdown';
import { Heading } from '@radix-ui/themes';
import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { SkeletonList } from '../components/ui/SkeletonList';
import { ListToolbar } from '../components/ui/ListToolbar';
import { MetadataTable, type MetadataRow } from '../components/ui/MetadataTable';
import { useCursorList } from '../hooks/useCursorList';
import { useListFilter } from '../hooks/useListFilter';
import type { Skill } from '../api/types';

export function SkillsPage() {
  const {
    items: skills,
    loading,
    error,
    hasMore,
    loadMore,
  } = useCursorList<Skill>('/api/v1/skills');

  const {
    filtered: visibleSkills,
    query,
    setQuery,
    sortDir,
    setSortDir,
    isFiltering,
  } = useListFilter(skills, {
    match: (s, q) => {
      if (s.name.toLowerCase().includes(q)) return true;
      if (s.description && s.description.toLowerCase().includes(q)) return true;
      return false;
    },
    compare: (a, b) => a.name.localeCompare(b.name),
  });

  return (
    <div>
      <Heading as="h1" size="6">
        Skills
      </Heading>
      <ListToolbar
        query={query}
        onQueryChange={setQuery}
        sortDir={sortDir}
        onSortDirChange={setSortDir}
        searchLabel="Skills"
        testId="skills-toolbar"
      />
      {error ? (
        <EmptyState
          icon="⚠️"
          title="Failed to load skills"
          description={error}
          testId="skills-error"
        />
      ) : loading && skills.length === 0 ? (
        <SkeletonList count={3} testId="skills-skeleton" />
      ) : visibleSkills.length === 0 && !error ? (
        <EmptyState
          icon={isFiltering ? '🔍' : '🧠'}
          title={isFiltering ? 'No skills match your search' : 'No skills registered'}
          description={
            isFiltering
              ? `No skills matched "${query}". Try clearing the search.`
              : "Drop a SKILL.md into your workspace's skills/ folder to register one."
          }
          testId="skills-empty"
        />
      ) : (
        visibleSkills.map((skill) => {
          const rows: MetadataRow[] = [
            {
              label: 'Name',
              value: (
                <Link
                  to={`/skills/${encodeURIComponent(skill.name)}`}
                  data-testid={`skill-link-${skill.name}`}
                >
                  {skill.name}
                </Link>
              ),
            },
          ];
          return (
            <Card key={skill.name} title={`Skill · ${skill.name}`}>
              <MetadataTable rows={rows} />
              {skill.description && (
                <div className="nt-markdown" data-testid={`skill-description-${skill.name}`}>
                  <Markdown source={skill.description} />
                </div>
              )}
              <div>
                <Link to={`/skills/${encodeURIComponent(skill.name)}`}>
                  <Button variant="soft" data-testid={`skill-view-${skill.name}`}>
                    View
                  </Button>
                </Link>
              </div>
            </Card>
          );
        })
      )}
      {hasMore && (
        <div>
          <Button
            variant="soft"
            onClick={loadMore}
            disabled={loading}
            data-testid="skills-load-more"
          >
            {loading ? 'Loading...' : 'Load More'}
          </Button>
        </div>
      )}
    </div>
  );
}
