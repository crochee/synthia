//! [`JsonlSessionSearch`] — the in-memory index over the JSONL
//! store this crate writes: scan on demand, index what changed,
//! answer queries from memory.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;

use super::{
    query::{
        EntryHit,
        SearchError,
        SearchQuery,
        SessionHit,
        SessionSearch,
        SharedSessionSearch,
    },
    score::{entry_score, snippet},
    text::message_text,
};
use crate::fold_log_surface;

/// One indexed entry: the searchable text plus where it came from.
#[derive(Debug, Clone)]
struct IndexedEntry {
    seq: u64,
    timestamp: Option<String>,
    /// Lowercased text, kept for scoring and snippets.
    text: String,
}

/// A session's address in the store: the owning user *and* the session id.
///
/// The id alone is not a key. Two users may pick the same one — a
/// default-named `main`, a timestamp, a client-supplied id — and an index
/// that folded them together would answer one tenant with the other's
/// transcript. The two parts are kept as two fields rather than joined
/// into one string, so no id containing the separator can forge a
/// collision.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SessionKey {
    user_id: String,
    session_id: String,
}

/// One indexed session.
#[derive(Debug, Clone, Default)]
struct SessionIndex {
    entries: Vec<IndexedEntry>,
    /// Size of `events.jsonl` when it was indexed — the change signal.
    source_bytes: u64,
}

/// [`SessionSearch`] over the JSONL layout this crate writes.
///
/// The index is in memory; [`SessionSearch::sync`] re-reads only the logs
/// whose size changed.
pub struct JsonlSessionSearch {
    root: PathBuf,
    /// Sessions the caller asked to forget, keyed the same way as the
    /// index. A tombstone keeps a log that still exists on disk from
    /// resurrecting itself on the next [`SessionSearch::sync`] — a
    /// deletion that has not been physically erased yet must still be
    /// unsearchable.
    removed: parking_lot::RwLock<std::collections::HashSet<SessionKey>>,
    index: parking_lot::RwLock<HashMap<SessionKey, SessionIndex>>,
}

impl std::fmt::Debug for JsonlSessionSearch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonlSessionSearch")
            .field("root", &self.root)
            .field("sessions", &self.index.read().len())
            .finish()
    }
}

impl JsonlSessionSearch {
    /// Index the sessions under `root` — the same directory
    /// [`SessionRegistry`](crate::manager::SessionRegistry) is built over
    /// (`<root>/<user>/<session>/events.jsonl`).
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            removed: parking_lot::RwLock::new(std::collections::HashSet::new()),
            index: parking_lot::RwLock::new(HashMap::new()),
        }
    }

    /// The directory being indexed.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// How many sessions are indexed right now (no IO).
    #[must_use]
    pub fn indexed_sessions(&self) -> usize {
        self.index.read().len()
    }

    /// Scan the store on the blocking pool: the IO is `std::fs`, so the
    /// async signature stays non-blocking without a runtime-specific API.
    ///
    /// A `user` restricts the walk to that user's directory, so a
    /// per-tenant search never reads another tenant's logs off disk. The
    /// `known` sizes are the same restriction: indexes are remembered per
    /// [`SessionKey`], and only the ones in the scanned scope can be hit.
    async fn scan(
        &self,
        user: Option<String>,
    ) -> Result<Vec<(SessionKey, SessionIndex)>, SearchError> {
        let root = self.root.clone();
        let known: HashMap<SessionKey, u64> = self
            .index
            .read()
            .iter()
            .filter(|(key, _)| {
                user.as_ref().is_none_or(|user| key.user_id == *user)
            })
            .map(|(key, index)| (key.clone(), index.source_bytes))
            .collect();
        let forgotten: std::collections::HashSet<SessionKey> =
            self.removed.read().clone();
        tokio::task::spawn_blocking(move || {
            scan_store(&root, user.as_deref(), &known, &forgotten)
        })
        .await
        .map_err(|e| SearchError::Io(format!("join scan: {e}")))?
    }
}

/// Walk `<root>/<user>/<session>/events.jsonl`, skipping logs whose size
/// matches `known`.
///
/// `user` selects one user's directory; `None` walks every user. In both
/// cases the owning user is read from the path and travels with the entry,
/// so the index — and a later query — can tell two users' identical
/// session ids apart.
///
/// The selection compares names from the root's own directory listing
/// rather than joining `<root>/<user>` directly, so a user id that
/// contains a separator or `..` can only ever fail to match — it cannot
/// name a path outside the store.
fn scan_store(
    root: &Path,
    user: Option<&str>,
    known: &HashMap<SessionKey, u64>,
    forgotten: &std::collections::HashSet<SessionKey>,
) -> Result<Vec<(SessionKey, SessionIndex)>, SearchError> {
    let mut found = Vec::new();
    let users = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        // A store that does not exist yet is empty, not an error: the
        // search index of a fresh deployment should not fail to sync.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(e) => {
            return Err(SearchError::Io(format!(
                "read {}: {e}",
                root.display()
            )));
        }
    };
    for owner in users.flatten() {
        if !owner.path().is_dir() {
            continue;
        }
        let user_id = owner.file_name().to_string_lossy().to_string();
        if user.is_some_and(|wanted| wanted != user_id) {
            continue;
        }
        let sessions = match std::fs::read_dir(owner.path()) {
            Ok(entries) => entries,
            Err(e) => {
                return Err(SearchError::Io(format!(
                    "read {}: {e}",
                    owner.path().display()
                )));
            }
        };
        for session in sessions.flatten() {
            let path = session.path().join("events.jsonl");
            let Ok(metadata) = path.metadata() else {
                continue;
            };
            let key = SessionKey {
                user_id: user_id.clone(),
                session_id: session.file_name().to_string_lossy().to_string(),
            };
            if forgotten.contains(&key) {
                continue;
            }
            if known.get(&key) == Some(&metadata.len()) {
                continue;
            }
            let raw = std::fs::read_to_string(&path).map_err(|e| {
                SearchError::Io(format!("read {}: {e}", path.display()))
            })?;
            found.push((key, index_log(&raw, metadata.len())?));
        }
    }
    Ok(found)
}

/// Fold one log's text into searchable entries.
fn index_log(
    raw: &str,
    source_bytes: u64,
) -> Result<SessionIndex, SearchError> {
    let rows: Vec<serde_json::Value> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let folded = fold_log_surface(&rows);
    let mut entries = Vec::new();
    for (position, message) in folded.messages.iter().enumerate() {
        let text = message_text(message);
        if text.trim().is_empty() {
            continue;
        }
        let seq = folded
            .surface_seqs
            .get(position)
            .copied()
            .unwrap_or(position as u64 + 1);
        entries.push(IndexedEntry {
            seq,
            timestamp: message
                .get("ts")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    rows.iter()
                        .find(|row| {
                            row.get("seq").and_then(serde_json::Value::as_u64)
                                == Some(seq)
                        })
                        .and_then(|row| {
                            row.get("ts")
                                .and_then(|value| value.as_str())
                                .map(str::to_string)
                        })
                }),
            text: text.to_lowercase(),
        });
    }
    Ok(SessionIndex {
        entries,
        source_bytes,
    })
}

/// The `sync` half of [`SessionSearch`], shared by the trait impl.
///
/// `user` narrows the refresh to one user's directory, so a scoped query
/// never walks — or reads — another user's logs.
async fn sync_index(
    backend: &JsonlSessionSearch,
    user: Option<String>,
) -> Result<usize, SearchError> {
    let scanned = backend.scan(user).await?;
    {
        let mut index = backend.index.write();
        for (key, session) in scanned {
            index.insert(key, session);
        }
    }
    Ok(backend.index.read().len())
}

/// Whether an indexed session is allowed to answer `query`.
///
/// Both filters are exact: a session the query did not ask for is not a
/// lower-ranked hit, it is not a hit.
fn in_scope(query: &SearchQuery, key: &SessionKey) -> bool {
    query
        .user_id
        .as_ref()
        .is_none_or(|user| user == &key.user_id)
        && query
            .session_id
            .as_ref()
            .is_none_or(|wanted| wanted == &key.session_id)
}

#[async_trait]
impl SessionSearch for JsonlSessionSearch {
    async fn search_sessions(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<SessionHit>, SearchError> {
        sync_index(self, query.user_id.clone()).await?;
        let terms = query.terms();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let index = self.index.read();
        let mut hits: Vec<SessionHit> = index
            .iter()
            .filter(|(key, _)| in_scope(query, key))
            .filter_map(|(key, session)| {
                let mut matched = 0usize;
                let mut total = 0.0f32;
                let mut top: Option<EntryHit> = None;
                for entry in &session.entries {
                    let Some(score) = entry_score(&entry.text, &terms) else {
                        continue;
                    };
                    matched += 1;
                    total += score;
                    let candidate = EntryHit {
                        session_id: key.session_id.clone(),
                        seq: entry.seq,
                        timestamp: entry.timestamp.clone(),
                        snippet: snippet(&entry.text, &terms, 60),
                        score,
                    };
                    if top
                        .as_ref()
                        .is_none_or(|best| candidate.score > best.score)
                    {
                        top = Some(candidate);
                    }
                }
                (matched > 0).then(|| SessionHit {
                    session_id: key.session_id.clone(),
                    score: total,
                    matched_entries: matched,
                    top,
                })
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.session_id.cmp(&b.session_id))
        });
        hits.truncate(query.effective_limit());
        Ok(hits)
    }

    async fn search_entries(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<EntryHit>, SearchError> {
        sync_index(self, query.user_id.clone()).await?;
        let terms = query.terms();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let index = self.index.read();
        let mut hits: Vec<EntryHit> = index
            .iter()
            .filter(|(key, _)| in_scope(query, key))
            .flat_map(|(key, session)| {
                session
                    .entries
                    .iter()
                    .filter_map(|entry| {
                        entry_score(&entry.text, &terms).map(|score| EntryHit {
                            session_id: key.session_id.clone(),
                            seq: entry.seq,
                            timestamp: entry.timestamp.clone(),
                            snippet: snippet(&entry.text, &terms, 60),
                            score,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.session_id.cmp(&b.session_id))
                .then_with(|| a.seq.cmp(&b.seq))
        });
        hits.truncate(query.effective_limit());
        Ok(hits)
    }

    async fn sync(&self) -> Result<usize, SearchError> {
        sync_index(self, None).await
    }

    async fn remove(
        &self,
        user_id: &str,
        session_id: &str,
    ) -> Result<bool, SearchError> {
        let key = SessionKey {
            user_id: user_id.to_string(),
            session_id: session_id.to_string(),
        };
        let was_indexed = self.index.write().remove(&key).is_some();
        self.removed.write().insert(key);
        Ok(was_indexed)
    }
}

/// Build a [`SharedSessionSearch`] over a JSONL store.
#[must_use]
pub fn jsonl_session_search(root: impl Into<PathBuf>) -> SharedSessionSearch {
    Arc::new(JsonlSessionSearch::new(root))
}
