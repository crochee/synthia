//! Retriever adapters — the two corpus domains.
//!
//! Memory and sessions already own retrieval (the memory tier's
//! `recall`, the session full-text index), so their engines
//! *delegate* instead of re-indexing the corpus. Both implement
//! [`ErasedEngine`] directly and join the cross-domain merge with
//! their own scores.
//!
//! Tenancy lives here and only here: the session engine reads the
//! requesting user from [`QueryContext::extra`] under
//! [`EXTRA_USER_SCOPE`] — a host routing fact the *server* sets.
//! No `synthia-*` library crate knows about users.
use synthia::{
    context::{Memory, MemoryEntry},
    search::{ErasedEngine, Hit, QueryContext},
    session::{SearchQuery, SessionHit, SharedSessionSearch},
};

use super::EXTRA_USER_SCOPE;

/// Title for a memory hit: the frontmatter `name` when the entry
/// was projected from a file, otherwise the first content line.
fn memory_title(entry: &MemoryEntry) -> String {
    if let Some(name) = entry
        .metadata
        .as_ref()
        .and_then(|m| m.get("name"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return name.chars().take(120).collect();
    }
    entry
        .content
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.chars().take(120).collect())
        .unwrap_or_else(|| entry.id.clone())
}

fn memory_preview(entry: &MemoryEntry) -> Option<String> {
    let first = entry
        .content
        .lines()
        .find(|l| !l.trim().is_empty())?
        .to_string();
    Some(first.chars().take(200).collect())
}

/// The memory domain: delegates to [`Memory::recall`], so whichever
/// backend the deployment wired (files, SQLite FTS5, in-memory)
/// answers with its own ranking.
pub(crate) struct MemoryEngine {
    memory: std::sync::Arc<dyn Memory>,
}

impl MemoryEngine {
    pub(crate) fn new(memory: std::sync::Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait::async_trait]
impl ErasedEngine for MemoryEngine {
    async fn search_erased(&self, ctx: &QueryContext) -> Vec<Hit> {
        let entries = match self.memory.recall(&ctx.text, ctx.top_k).await {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(error = %e, "search: memory recall failed");
                return Vec::new();
            }
        };
        // `recall` answers best-first with an opaque score; rank
        // position is the only comparable signal we have, so map it
        // monotonically onto (0, 1].
        entries
            .into_iter()
            .enumerate()
            .map(|(i, entry)| Hit {
                item_idx: i,
                domain: String::new(),
                id: entry.id.clone(),
                title: memory_title(&entry),
                score: 1.0 / (1.0 + i as f32),
                bm25: 0.0,
                vector: 0.0,
                reasons: vec!["long-term memory recall".into()],
                preview: memory_preview(&entry),
            })
            .collect()
    }

    fn len(&self) -> usize {
        0
    }

    fn type_name(&self) -> &'static str {
        "synthia_server::search::MemoryEngine"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn session_title(hit: &SessionHit) -> String {
    hit.top
        .as_ref()
        .and_then(|t| t.timestamp.clone())
        .unwrap_or_else(|| {
            format!(
                "session {}",
                hit.session_id.chars().take(8).collect::<String>()
            )
        })
}

/// The session domain: delegates to [`SharedSessionSearch`]. An
/// unscoped query searches the whole store — right for the agent
/// tool in a single-tenant deployment; the HTTP handler always
/// sets [`EXTRA_USER_SCOPE`] so one tenant's request is answered
/// only with that tenant's transcripts.
pub(crate) struct SessionEngine {
    search: SharedSessionSearch,
}

impl SessionEngine {
    pub(crate) fn new(search: SharedSessionSearch) -> Self {
        Self { search }
    }
}

#[async_trait::async_trait]
impl ErasedEngine for SessionEngine {
    async fn search_erased(&self, ctx: &QueryContext) -> Vec<Hit> {
        let mut query = SearchQuery::new(&ctx.text).with_limit(ctx.top_k);
        if let Some(user) = ctx
            .extra
            .get(EXTRA_USER_SCOPE)
            .and_then(|v| v.as_str())
            .filter(|u| !u.is_empty())
        {
            query = query.in_user(user);
        }
        let hits = match self.search.search_sessions(&query).await {
            Ok(hits) => hits,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "search: session full-text search failed"
                );
                return Vec::new();
            }
        };
        hits.into_iter()
            .enumerate()
            .map(|(i, hit)| Hit {
                item_idx: i,
                domain: String::new(),
                id: hit.session_id.clone(),
                title: session_title(&hit),
                score: hit.score,
                bm25: 0.0,
                vector: 0.0,
                reasons: vec![format!(
                    "{} matched transcript entries",
                    hit.matched_entries
                )],
                preview: hit.top.as_ref().map(|t| t.snippet.clone()),
            })
            .collect()
    }

    fn len(&self) -> usize {
        0
    }

    fn type_name(&self) -> &'static str {
        "synthia_server::search::SessionEngine"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
