import { useMemo, useState, type FormEvent } from 'react';
import { Link } from 'react-router-dom';
import { Markdown } from '../components/chat/Markdown';
import { Button } from '../components/ui/Button';
import { EmptyState } from '../components/ui/EmptyState';
import { Input } from '../components/ui/Input';
import { Modal } from '../components/ui/Modal';
import { SkeletonList } from '../components/ui/SkeletonList';
import { ListToolbar } from '../components/ui/ListToolbar';
import { ENTITY_THEMES, PageHero, PageShell, StatusPill, CardGrid } from '../components/ui/visual';
import { api } from '../api/client';
import { useToast } from '../hooks/useToast';
import { useCursorList } from '../hooks/useCursorList';
import { useListFilter } from '../hooks/useListFilter';
import type {
  CreateSkillRequest,
  CreateSkillResponse,
  ReloadSkillsResponse,
  Skill,
} from '../api/types';

const SKILL_THEME = ENTITY_THEMES.skill;

/** Template pre-filled into the Create Skill modal. Mirrors the
 *  on-disk SKILL.md contract: YAML frontmatter with `name`
 *  (required by the loader to equal the directory name) and
 *  `description`, then the markdown body. */
const NEW_SKILL_TEMPLATE = `---
name: my-skill
description: One line the model reads when deciding to invoke this skill.
---

## Purpose

Describe the workflow this skill encodes.

## Steps

1. …
`;

interface CreateFormState {
  name: string;
  content: string;
}

const EMPTY_CREATE_FORM: CreateFormState = {
  name: '',
  content: NEW_SKILL_TEMPLATE,
};

/**
 * Skills registry. Each row is a coloured card linking to
 * the read-only inspector at `/skills/:name`. The hero
 * block at the top carries the page title + total count so
 * the user can see at a glance how many skills are loaded
 * without scrolling back up.
 *
 * The full skill lifecycle is wired here: create (writes a
 * SKILL.md under `<workspace>/.agents/skills/`), delete
 * (removes the skill directory), and reload (rescans the
 * directory so out-of-band edits land without a restart).
 */
export function SkillsPage() {
  const {
    items: skills,
    loading,
    error,
    hasMore,
    loadMore,
    refresh,
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

  const [createOpen, setCreateOpen] = useState(false);
  const [createForm, setCreateForm] = useState<CreateFormState>(EMPTY_CREATE_FORM);
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  /** The skill awaiting delete confirmation — a styled dialog
   *  instead of `window.confirm`, matching the Agents page. */
  const [confirmTarget, setConfirmTarget] = useState<Skill | null>(null);
  const [pendingDelete, setPendingDelete] = useState<Record<string, boolean>>({});
  const [reloading, setReloading] = useState(false);
  const toast = useToast();

  const total = skills.length;

  const closeCreate = (): void => {
    setCreateOpen(false);
    setCreateError(null);
    setCreateForm(EMPTY_CREATE_FORM);
  };

  const submitCreate = async (): Promise<void> => {
    setCreating(true);
    setCreateError(null);
    try {
      const body: CreateSkillRequest = {
        name: createForm.name.trim(),
        content: createForm.content,
      };
      const created = await api.post<CreateSkillResponse>('/api/v1/skills', body);
      toast.push({
        variant: 'success',
        message: `Created skill "${created.name}" at ${created.path}.`,
      });
      closeCreate();
      await refresh();
    } catch (e) {
      setCreateError((e as Error).message);
    } finally {
      setCreating(false);
    }
  };

  const deleteSkill = async (skill: Skill): Promise<void> => {
    setConfirmTarget(null);
    setPendingDelete((p) => ({ ...p, [skill.name]: true }));
    try {
      await api.del(`/api/v1/skills/${encodeURIComponent(skill.name)}`);
      toast.push({
        variant: 'success',
        message: `Deleted skill "${skill.name}".`,
      });
      await refresh();
    } catch (e) {
      toast.push({ variant: 'error', message: (e as Error).message });
    } finally {
      setPendingDelete((p) => ({ ...p, [skill.name]: false }));
    }
  };

  const reloadSkills = async (): Promise<void> => {
    setReloading(true);
    try {
      const resp = await api.post<ReloadSkillsResponse>('/api/v1/skills/reload');
      toast.push({
        variant: 'success',
        message: `Rescanned skills directory — ${resp.count} found.`,
      });
      await refresh();
    } catch (e) {
      toast.push({ variant: 'error', message: (e as Error).message });
    } finally {
      setReloading(false);
    }
  };

  // Re-use the SKILL_THEME tone across every card and the
  // hero so the page reads as one cohesive visual block.
  const heroPills = useMemo(
    () => (
      <>
        <StatusPill tone="purple" testId="skills-total-pill">
          {`${total} loaded`}
        </StatusPill>
        {isFiltering && (
          <StatusPill tone="cyan" withDot>
            filtering · {visibleSkills.length}/{total}
          </StatusPill>
        )}
      </>
    ),
    [total, isFiltering, visibleSkills.length],
  );

  return (
    <PageShell testId="skills-page">
      <PageHero
        glyph={SKILL_THEME.glyph}
        heroModifier={SKILL_THEME.heroModifier}
        title="Skills"
        subtitle="Reusable Markdown playbooks the agent can load on demand. Create one here, drop a SKILL.md into the workspace yourself, or reload to pick up out-of-band edits."
        pills={heroPills}
        meta={
          <div className="nt-vis-meta-cluster">
            <div className="nt-vis-meta-cluster__row">
              <span>Source</span>
              <span>workspace / SKILL.md</span>
            </div>
            <div className="nt-vis-meta-cluster__row">
              <span>API</span>
              <span>/api/v1/skills</span>
            </div>
          </div>
        }
        testId="skills-hero"
      />

      <ListToolbar
        query={query}
        onQueryChange={setQuery}
        sortDir={sortDir}
        onSortDirChange={setSortDir}
        searchLabel="Skills"
        testId="skills-toolbar"
      >
        <Button
          variant="soft"
          onClick={() => void reloadSkills()}
          disabled={reloading}
          data-testid="skills-reload"
        >
          {reloading ? 'Reloading...' : 'Reload'}
        </Button>
        <Button variant="solid" onClick={() => setCreateOpen(true)} data-testid="skills-create">
          + Create Skill
        </Button>
      </ListToolbar>

      {error ? (
        <EmptyState
          icon="⚠️"
          title="Failed to load skills"
          description={error}
          testId="skills-error"
        />
      ) : loading && skills.length === 0 ? (
        <SkeletonList count={6} testId="skills-skeleton" />
      ) : visibleSkills.length === 0 && !error ? (
        <EmptyState
          icon={isFiltering ? '🔍' : '🧠'}
          title={isFiltering ? 'No skills match your search' : 'No skills registered'}
          description={
            isFiltering
              ? `No skills matched "${query}". Try clearing the search.`
              : 'Create a skill here, or drop a SKILL.md into your workspace\u2019s skills/ folder and hit Reload.'
          }
          testId="skills-empty"
          action={
            !isFiltering ? (
              <Button
                variant="solid"
                onClick={() => setCreateOpen(true)}
                data-testid="skills-create-empty"
              >
                + Create Skill
              </Button>
            ) : undefined
          }
        />
      ) : (
        <CardGrid testId="skills-grid">
          {visibleSkills.map((skill) => {
            const href = `/skills/${encodeURIComponent(skill.name)}`;
            return (
              <Link
                key={skill.name}
                to={href}
                className={`nt-vis-card ${SKILL_THEME.cardModifier}`}
                data-testid={`skill-card-${skill.name}`}
              >
                <div className="nt-vis-card__header">
                  <div className="nt-vis-card__icon" aria-hidden>
                    {SKILL_THEME.glyph}
                  </div>
                  <div className="nt-vis-card__main">
                    <h3 className="nt-vis-card__name" title={skill.name}>
                      <code>{skill.name}</code>
                    </h3>
                    <span className="nt-vis-card__subtitle">Playbook</span>
                  </div>
                </div>
                {skill.description ? (
                  <div
                    className="nt-vis-card__desc nt-markdown"
                    data-testid={`skill-description-${skill.name}`}
                  >
                    <Markdown source={skill.description} />
                  </div>
                ) : (
                  <p className="nt-vis-card__desc" style={{ color: 'var(--text-muted)' }}>
                    No description provided.
                  </p>
                )}
                <div className="nt-vis-card__footer">
                  <div className="nt-vis-card__pills">
                    <StatusPill tone="purple">SKILL.md</StatusPill>
                  </div>
                  <div
                    className="nt-vis-card__actions"
                    onClick={(e) => {
                      // Keep the delete click inside the card —
                      // without this the wrapping link swallows it
                      // and the user lands on the detail page.
                      e.preventDefault();
                      e.stopPropagation();
                    }}
                  >
                    <Button
                      variant="soft"
                      color="red"
                      size="1"
                      onClick={() => setConfirmTarget(skill)}
                      disabled={!!pendingDelete[skill.name]}
                      data-testid={`skill-delete-${skill.name}`}
                    >
                      {pendingDelete[skill.name] ? 'Deleting...' : 'Delete'}
                    </Button>
                  </div>
                </div>
              </Link>
            );
          })}
        </CardGrid>
      )}

      {hasMore && (
        <div style={{ display: 'flex', justifyContent: 'center' }}>
          <Button
            variant="soft"
            onClick={loadMore}
            disabled={loading}
            data-testid="skills-load-more"
          >
            {loading ? 'Loading...' : 'Load More Skills'}
          </Button>
        </div>
      )}

      <Modal
        open={createOpen}
        onClose={closeCreate}
        title="Create skill"
        testId="skill-create-modal"
        footer={
          <>
            <Button variant="soft" onClick={closeCreate} data-testid="skill-cancel">
              Cancel
            </Button>
            <Button
              variant="solid"
              onClick={() => void submitCreate()}
              disabled={creating || !createForm.name.trim() || !createForm.content.trim()}
              loading={creating}
              data-testid="skill-submit"
            >
              {creating ? 'Creating...' : 'Create'}
            </Button>
          </>
        }
      >
        <form
          onSubmit={(e: FormEvent) => {
            e.preventDefault();
            void submitCreate();
          }}
          className="nt-form"
          data-testid="skill-create-form"
        >
          <Input
            label="Name"
            value={createForm.name}
            onChange={(e) => setCreateForm((f) => ({ ...f, name: e.target.value }))}
            placeholder="my-skill"
            required
            data-testid="skill-name"
          />
          <label className="nt-form__label">
            <span>SKILL.md content (frontmatter + body)</span>
            <textarea
              rows={12}
              value={createForm.content}
              onChange={(e) => setCreateForm((f) => ({ ...f, content: e.target.value }))}
              spellCheck={false}
              data-testid="skill-content"
            />
          </label>
          <p className="nt-form__hint">
            The <code>name</code> in the frontmatter must match the Name field — the loader rejects
            a mismatch.
          </p>
          {createError && (
            <p className="nt-form__error" role="alert" data-testid="skill-submit-error">
              <code>{createError}</code>
            </p>
          )}
          <button type="submit" hidden tabIndex={-1} aria-hidden="true" />
        </form>
      </Modal>

      <Modal
        open={confirmTarget !== null}
        onClose={() => setConfirmTarget(null)}
        title="Delete skill"
        testId="skill-delete-modal"
        footer={
          <>
            <Button
              variant="soft"
              onClick={() => setConfirmTarget(null)}
              data-testid="skill-delete-cancel"
            >
              Cancel
            </Button>
            <Button
              variant="solid"
              color="red"
              onClick={() => confirmTarget && void deleteSkill(confirmTarget)}
              data-testid="skill-delete-confirm"
            >
              Delete
            </Button>
          </>
        }
      >
        <p>
          Delete skill <strong>{confirmTarget?.name}</strong>? This removes the skill&apos;s
          directory from disk and cannot be undone.
        </p>
      </Modal>
    </PageShell>
  );
}
