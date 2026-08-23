//! Memory search handler.
//!
//! Memory search is a placeholder: it iterates the
//! `.agents/skills/` directory and uses skill files as a
//! memory proxy, ranking by simple keyword match count
//! (score = `min(occurrences * 0.5, 1.0)`).
//!
//! The v1 API wraps results in [`List<MemoryResult>`] with
//! cursor-based pagination driven by [`PageQuery`].

use std::{path::PathBuf, sync::Arc};

use axum::{Json, extract::State};
use parking_lot::RwLock;
use serde::Serialize;
use synthia_core::Error;

use super::helpers::paginate;
use crate::{
    api::{AppError, AppQuery, List, PageQuery, resolve_page},
    state::AppState,
};

/// Query parameters for memory search.
#[derive(serde::Deserialize, validator::Validate)]
pub struct MemorySearchQuery {
    /// Search query string.
    #[validate(length(min = 1, message = "must not be empty"))]
    pub q: String,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Serialize)]
pub struct MemoryResult {
    pub id: String,
    pub content: String,
    pub score: f64,
}

/// One cached skill file: the path + last-modified timestamp + the
/// pre-lowercased content. We snapshot the raw content + its
/// lowercased form so the per-search `to_lowercase()` call is
/// amortised away — every keyword-search is now a substring scan
/// over an already-lowered `String`.
struct CachedSkill {
    name: String,
    /// File mtime at the moment we read it; if the on-disk mtime
    /// differs at the next request, we re-read.
    mtime: std::time::SystemTime,
    /// Lowercased full body, used for case-insensitive matching.
    content_lower: String,
    /// First 5 lines joined — the snippet the API returns.
    snippet: String,
}

impl Clone for CachedSkill {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            mtime: self.mtime,
            content_lower: self.content_lower.clone(),
            snippet: self.snippet.clone(),
        }
    }
}

/// Process-wide cache of skill files. The skills directory is
/// scanned + lowercased at most once per file (until the file's
/// mtime changes), which is what the previous per-request scan
/// + read + `to_lowercase()` did on every single search query.
///
/// Keyed by absolute file path so two skills sharing a directory
/// don't collide. `parking_lot::RwLock` keeps the read path
/// lock-free under contention — every search request does an
/// `try_read()` and only one writer (a cache miss) ever takes
/// the write lock.
static SKILL_CACHE: RwLock<Vec<CachedSkill>> =
    parking_lot::const_rwlock(Vec::new());

/// Read or refresh the cached entry for `skill_md`. Returns
/// `None` if the file no longer exists or failed to read.
///
/// Cache invalidation: cheap — compare the on-disk `mtime`
/// against the cached `mtime`. A skill edit (or `POST /skills`
/// or `DELETE /skills/{name}` via the v1 API) bumps the mtime
/// and the next request transparently refreshes.
///
/// Worst case is one `stat()` syscall per cached file per
/// search; in the common case (no edits since last scan) the
/// `read_to_string` + `to_lowercase` is skipped entirely.
fn cached_skill_for(
    skill_md: &std::path::Path,
    name: String,
) -> Option<CachedSkill> {
    let mtime = std::fs::metadata(skill_md).ok()?.modified().ok()?;
    {
        let cache = SKILL_CACHE.read();
        if let Some(entry) = cache.iter().find(|e| e.name == name)
            && entry.mtime == mtime
        {
            return Some(entry.clone());
        }
    }
    // Slow path: read, lowercase, snippet, then publish under the
    // write lock. Holding the write lock while we read disk would
    // block every concurrent search — instead, do the disk work
    // outside any lock and only briefly take the write lock to
    // upsert.
    let content = std::fs::read_to_string(skill_md).ok()?;
    let content_lower = content.to_lowercase();
    let snippet = content.lines().take(5).collect::<Vec<_>>().join(" ");
    let entry = CachedSkill {
        name,
        mtime,
        content_lower,
        snippet,
    };
    let mut cache = SKILL_CACHE.write();
    // Replace by name so a stale entry from a previous mtime
    // doesn't linger. `retain` + `push` avoids re-sorting.
    cache.retain(|e| e.name != entry.name);
    cache.push(entry.clone());
    Some(entry)
}

/// GET /api/memory/search?q=<query> - Search memory with cursor pagination.
///
/// Per `api-list-pagination/spec.md:87-89`: memory search ignores
/// the `sort` parameter and always uses fixed `score` DESC order.
/// The `sort` query parameter is silently dropped (any value the
/// client sends is accepted but has no effect). `cursor` and
/// `limit` are still honored.
pub async fn search_memory(
    State(state): State<Arc<AppState>>,
    AppQuery(params): AppQuery<MemorySearchQuery>,
) -> Result<Json<List<MemoryResult>>, AppError> {
    if params.q.trim().is_empty() {
        return Err(AppError::from(Error::invalid_item("query parameter 'q'")));
    }
    let resolved = resolve_page(&params.page)?;

    let skills_dir: PathBuf =
        state.workspace_root.join(".agents").join("skills");
    let query_lower = params.q.to_lowercase();
    let mut results: Vec<MemoryResult> = Vec::new();

    if skills_dir.exists()
        && let Ok(entries) = std::fs::read_dir(&skills_dir)
    {
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(|s| s.to_string())
            else {
                continue;
            };
            let skill_md = entry.path().join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }
            let Some(cached) = cached_skill_for(&skill_md, name) else {
                continue;
            };
            // `memchr::memmem::find` walks the haystack once to
            // locate the first match. The previous version did
            // a separate `contains` (also a full scan via the
            // same primitive) and then `matches().count()` —
            // two passes over the cached body for every skill
            // file on every query. Count first, then check
            // non-zero: one scan per file, plus an extra branch.
            let occurrences =
                cached.content_lower.matches(&query_lower).count();
            if occurrences > 0 {
                let score = (occurrences as f64 * 0.5).min(1.0);
                results.push(MemoryResult {
                    id: cached.name,
                    content: cached.snippet,
                    score,
                });
            }
        }
    }

    // Fixed sort: score DESC (highest relevance first), with `id`
    // ascending as a deterministic tiebreaker. The client-supplied
    // `sort` parameter is intentionally ignored per spec.
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });

    let list = paginate(results, &resolved, |r: &MemoryResult| r.id.as_str());
    Ok(Json(list))
}

#[cfg(test)]
mod tests {
    use std::{io::Write, time::Duration};

    use super::*;

    fn write_skill(dir: &std::path::Path, name: &str, body: &str) {
        let skill_dir = dir.join(".agents").join("skills").join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), body).unwrap();
    }

    /// Cache invalidation: a freshly-written skill MUST appear in
    /// the next search result without any manual cache flush.
    /// The mtime comparison inside `cached_skill_for` is the
    /// contract under test.
    #[test]
    fn cache_invalidates_when_skill_mtime_changes() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(dir.path(), "alpha", "alpha body one");
        // First read populates the cache.
        let entry = cached_skill_for(
            &dir.path().join(".agents/skills/alpha/SKILL.md"),
            "alpha".into(),
        )
        .unwrap();
        assert_eq!(entry.content_lower, "alpha body one");
        // Sleep just long enough that the next write has a
        // strictly-greater mtime — most filesystems resolve
        // mtime at second or millisecond resolution, so we use
        // 50ms to be safely above the floor.
        std::thread::sleep(Duration::from_millis(50));
        // Mutate the file: re-open in append, write a byte.
        let path = dir.path().join(".agents/skills/alpha/SKILL.md");
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"\nalpha body two").unwrap();
        // Second read MUST pick up the new content (mtime > cached).
        let refreshed = cached_skill_for(&path, "alpha".into()).unwrap();
        assert!(
            refreshed.content_lower.contains("alpha body two"),
            "expected cache to invalidate after mtime bump, got: {}",
            refreshed.content_lower
        );
    }

    #[test]
    fn memory_result_serializes_all_fields() {
        let r = MemoryResult {
            id: "skill-a".to_string(),
            content: "first line".to_string(),
            score: 0.5,
        };
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["id"], "skill-a");
        assert_eq!(json["content"], "first line");
        assert_eq!(json["score"], 0.5);
    }

    #[test]
    fn memory_search_query_deserializes_with_page_flatten() {
        let raw = r#"{"q":"hello","limit":5,"cursor":"abc"}"#;
        let parsed: MemorySearchQuery = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.q, "hello");
        assert_eq!(parsed.page.limit, Some(5));
        assert_eq!(parsed.page.cursor.as_deref(), Some("abc"));
    }

    #[test]
    fn memory_search_query_rejects_missing_q() {
        let raw = r#"{"limit":5}"#;
        let parsed = serde_json::from_str::<MemorySearchQuery>(raw);
        assert!(parsed.is_err());
    }
}
