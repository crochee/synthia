//! Skill management handlers.
//!
//! Skills are stored under
//! `<workspace>/.agents/skills/<name>/SKILL.md`.
//!
//! Surface:
//! - `GET    /api/v1/skills`         — list (cursor-paginated)
//! - `GET    /api/v1/skills/{name}`  — detail (frontmatter + body)
//! - `POST   /api/v1/skills`         — create from raw SKILL.md content
//! - `DELETE /api/v1/skills/{name}`  — remove skill directory
//! - `POST   /api/v1/skills/reload`  — rescan the skills directory
//!
//! CRUD coverage was restored in turn 13 of the 2026-08-15
//! optimization pass to address Task 3 ("实现skill.tool、agent、
//! model的全生命周期管理") of the active goal. The per-skill
//! enable toggle was removed alongside the `Settings` subsystem
//! (now deleted); all loaded skills are considered enabled.

use std::{
    io::{ErrorKind, Write},
    path::Path as StdPath,
    sync::Arc,
};

use axum::{Json, extract::State};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use synthia_core::Error;

use super::helpers::paginate;
use crate::{
    api::{
        AppError,
        AppJson,
        AppPath,
        AppQuery,
        List,
        PageQuery,
        ResolvedPage,
        resolve_page,
        validate_resource_name,
        validate_sort,
    },
    state::AppState,
};

/// Sortable fields for the skills list endpoint.
const SKILL_SORT_WHITELIST: &[&str] = &["name", "created_at"];

#[derive(Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
}

/// Full detail returned by `GET /api/skills/:id`.
///
/// In addition to the short `description` (used in list rows), the
/// detail payload exposes the raw frontmatter map and the markdown
/// body so the UI can render the SKILL.md as structured metadata
/// plus rendered markdown — no second round-trip needed.
#[derive(Serialize)]
pub struct SkillDetail {
    pub name: String,
    pub description: String,
    pub path: String,
    /// Parsed YAML/JSON frontmatter of SKILL.md, with `name` /
    /// `description` / `metadata` keys flattened into a flat
    /// `key → value` map for easy table rendering on the client.
    pub frontmatter: serde_json::Map<String, serde_json::Value>,
    /// Markdown body (everything after the frontmatter closer).
    /// Empty string when the file has no body.
    pub body: String,
}

#[derive(Serialize)]
pub struct SkillDeleteResponse {
    pub name: String,
}

/// One cached skill list-row: name + directory mtime + parsed
/// short description. Frontend list pages render this directly,
/// so `GET /api/v1/skills` is the hottest read on the skills
/// subsystem and is what the cache was built for.
#[derive(Clone)]
struct CachedListEntry {
    name: String,
    /// Directory mtime at the moment we read SKILL.md; a different
    /// mtime at the next request triggers a re-read.
    mtime: std::time::SystemTime,
    description: String,
}

/// One cached skill detail: SKILL.md content + parsed
/// frontmatter + extracted description. The detail page renders
/// all three, so the cache collapses three `O(file_size)` parse
/// passes into one per file.
#[derive(Clone)]
struct CachedDetail {
    name: String,
    /// SKILL.md mtime at read time.
    mtime: std::time::SystemTime,
    description: String,
    frontmatter: serde_json::Map<String, serde_json::Value>,
    body: String,
}

/// Process-wide cache of `(name, description)` pairs for the
/// list endpoint. Invalidated by mtime comparison, so a skill
/// write / delete / reload is transparently reflected on the
/// next request without a manual flush. `HashMap` (not `Vec`)
/// so the per-skill lookup inside the list endpoint is O(1)
/// instead of an O(N) linear scan — the previous `Vec` version
/// did a `String` comparison per cached entry on every list
/// request. `LazyLock` because `HashMap::new` is not `const`,
/// so a `const_rwlock(HashMap::new())` static won't compile.
static LIST_CACHE: std::sync::LazyLock<
    RwLock<std::collections::HashMap<String, CachedListEntry>>,
> = std::sync::LazyLock::new(|| {
    parking_lot::const_rwlock(std::collections::HashMap::new())
});

/// Process-wide cache of full detail payloads, keyed the same
/// way as [`LIST_CACHE`] so a single invalidation passes through
/// to both. Frontend detail pages hit this on every mount and
/// when the user clicks a skill card — without it the user
/// re-pays the YAML parse + body extract on every navigation.
static DETAIL_CACHE: std::sync::LazyLock<
    RwLock<std::collections::HashMap<String, CachedDetail>>,
> = std::sync::LazyLock::new(|| {
    parking_lot::const_rwlock(std::collections::HashMap::new())
});

/// Read or refresh the cached `(name, description)` entry for one
/// skill directory. Returns `None` for file entries (non-directory
/// skills) — the list endpoint skips those, same as the previous
/// cold-path implementation.
///
/// `mtime` is pre-fetched by the caller (who already holds a
/// `read_dir` entry's `metadata()` result). This avoids a second
/// `metadata()` syscall per directory. `None` falls back to a
/// fresh stat, so this helper also works for call sites that
/// haven't stat'd the directory yet.
fn cached_list_entry_for_with_mtime(
    dir: &StdPath,
    mtime: Option<std::time::SystemTime>,
) -> Option<CachedListEntry> {
    let mtime = match mtime {
        Some(t) => t,
        None => std::fs::metadata(dir).ok()?.modified().ok()?,
    };
    let name = dir.file_name()?.to_str()?.to_string();
    {
        let cache = LIST_CACHE.read();
        if let Some(entry) = cache.get(&name)
            && entry.mtime == mtime
        {
            return Some(entry.clone());
        }
    }
    // Slow path: read SKILL.md, extract description, upsert.
    // We do the disk read OUTSIDE any lock so concurrent
    // `list_skills` calls don't stall on each other. We use
    // `extract_body` + `extract_description_from` instead of
    // `extract_description` directly so we skip the YAML
    // frontmatter parse that the list endpoint never needs.
    let skill_md = dir.join("SKILL.md");
    let description = if skill_md.exists() {
        std::fs::read_to_string(&skill_md)
            .map(|c| extract_description_from(extract_body(&c), &c))
            .unwrap_or_else(|_| "Skill directory".to_string())
    } else {
        "Skill directory".to_string()
    };
    let entry = CachedListEntry {
        name,
        mtime,
        description,
    };
    let mut cache = LIST_CACHE.write();
    // `HashMap::insert` overwrites in place — no separate
    // `retain + push` needed. The previous `Vec` version did
    // an O(N) scan per upsert; now it's O(1) hash + replace.
    cache.insert(entry.name.clone(), entry.clone());
    Some(entry)
}

/// Read or refresh the cached full-detail entry for one
/// SKILL.md. Returns `None` only when the file disappeared
/// between `exists()` and `read_to_string` (rare race); the
/// caller treats that as 404 just like a regular missing file.
fn cached_detail_for(skill_md: &StdPath, name: &str) -> Option<CachedDetail> {
    let mtime = std::fs::metadata(skill_md).ok()?.modified().ok()?;
    {
        let cache = DETAIL_CACHE.read();
        if let Some(entry) = cache.get(name)
            && entry.mtime == mtime
        {
            return Some(entry.clone());
        }
    }
    // Slow path: read SKILL.md, parse frontmatter, extract
    // description, capture body — all outside the lock so
    // concurrent `get_skill` calls for *different* skills don't
    // serialise on each other.
    let content = std::fs::read_to_string(skill_md).ok()?;
    let (frontmatter, body) = parse_skill_md(&content);
    // Reuse the body we just parsed instead of re-running
    // `extract_description(&content)` which would call
    // `parse_skill_md` a *second* time on the same content.
    let description = extract_description_from(&body, &content);
    let entry = CachedDetail {
        name: name.to_string(),
        mtime,
        description,
        frontmatter,
        body,
    };
    let mut cache = DETAIL_CACHE.write();
    // `HashMap::insert` overwrites in place — no separate
    // `retain + push` needed. The previous `Vec` version did
    // an O(N) scan per upsert; now it's O(1) hash + replace.
    cache.insert(entry.name.clone(), entry.clone());
    Some(entry)
}

/// Invalidate both the list and detail caches. Called after a
/// skill is created / deleted / reloaded so subsequent reads
/// re-scan. `reload_skills` uses this path; for `create_skill`
/// and `delete_skill` only the affected name changes, but a
/// wholesale clear is simpler than partial invalidation and keeps
/// the cache footprint bounded.
fn invalidate_list_cache() {
    LIST_CACHE.write().clear();
    DETAIL_CACHE.write().clear();
}

/// GET /api/skills - List all skills with cursor pagination.
pub async fn list_skills(
    State(state): State<Arc<AppState>>,
    AppQuery(page): AppQuery<PageQuery>,
) -> Result<Json<List<SkillInfo>>, AppError> {
    validate_sort(
        page.sort.as_deref().unwrap_or("name"),
        SKILL_SORT_WHITELIST,
    )?;
    let resolved = resolve_page(&page)?;

    let skills_dir = state.workspace_root.join(".agents").join("skills");
    let mut skills: Vec<(SkillInfo, std::time::SystemTime)> = Vec::new();

    // `read_dir` already errors when the directory doesn't exist,
    // so a separate `exists()` syscall would just duplicate the
    // kernel round-trip — the previous version did both.
    if let Ok(entries) = std::fs::read_dir(&skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                // File entries don't go in the list (kept
                // consistent with the previous behaviour).
                continue;
            }
            // Single metadata() call: passed to the cache helper
            // for mtime-based invalidation AND reused below for
            // the `created_at` sort key. The previous version
            // called metadata() twice per directory (once inside
            // cached_list_entry_for, once here).
            let dir_mtime = entry.metadata().and_then(|m| m.modified()).ok();
            let Some(cached) =
                cached_list_entry_for_with_mtime(&path, dir_mtime)
            else {
                continue;
            };
            // `created_at` falls back to the cache mtime (which
            // equals the dir mtime on the slow path) when the
            // caller doesn't have a fresh syscall result.
            let sort_mtime = dir_mtime.unwrap_or(cached.mtime);
            skills.push((
                SkillInfo {
                    name: cached.name,
                    description: cached.description,
                },
                sort_mtime,
            ));
        }
    }

    sort_skills(&mut skills, &resolved);

    let skills: Vec<SkillInfo> = skills.into_iter().map(|(s, _)| s).collect();
    let list = paginate(skills, &resolved, |s: &SkillInfo| s.name.as_str());
    Ok(Json(list))
}

/// Extract the SKILL.md content as `(frontmatter_json_map, body)`.
/// On any parse error (missing delimiters, malformed YAML/JSON),
/// falls back to `(empty_map, full_content)` so the UI still
/// receives something useful — a markdown body without
/// frontmatter is still a valid SKILL.md surface.
fn parse_skill_md(
    content: &str,
) -> (serde_json::Map<String, serde_json::Value>, String) {
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return (serde_json::Map::new(), content.to_string());
    }
    let raw_fm = parts[1].trim();
    let body = parts[2].trim_start_matches('\n').to_string();
    let value = serde_yaml::from_str::<serde_json::Value>(raw_fm)
        .or_else(|_| serde_json::from_str::<serde_json::Value>(raw_fm));
    let map = match value {
        Ok(serde_json::Value::Object(m)) => m,
        _ => serde_json::Map::new(),
    };
    (map, body)
}

/// Pull the markdown body out of `content` without paying for
/// the YAML/JSON frontmatter parse. Returns the body when
/// frontmatter delimiters are present, or the full content
/// otherwise — matching the `body` leg of [`parse_skill_md`].
///
/// Used by the list endpoint, which only needs the body's first
/// non-empty line; paying for the frontmatter `serde_yaml`
/// deserializer just to throw it away was wasted work on every
/// cache miss (and the YAML parse is the single biggest cost
/// in the SKILL.md read path).
fn extract_body(content: &str) -> &str {
    let parts: [&str; 3] = content
        .splitn(3, "---")
        .collect::<Vec<_>>()
        .try_into()
        .unwrap_or([content, "", ""]);
    if parts[0].is_empty() && parts[2].is_empty() {
        // No delimiters found; the entire content was the single
        // segment returned by splitn.
        content
    } else {
        // `splitn(3, "---")` returns [pre, fm, body]. We want the
        // body segment, with leading newlines stripped to match
        // the `parse_skill_md` body format the previous code
        // produced.
        parts[2].trim_start_matches('\n')
    }
}

/// Pull the first non-empty line(s) of the markdown body as a
/// short description. Falls back to the first non-empty line of the
/// raw file when the body is empty (e.g. malformed frontmatter).
///
/// Accepts an already-extracted body slice (see
/// [`extract_body`]) so the list endpoint can avoid paying for a
/// YAML parse it would immediately discard. Pass the original
/// content when the body hasn't been pre-extracted yet — the
/// function will do the body extraction internally.
fn extract_description_from(body: &str, fallback: &str) -> String {
    let source = if body.is_empty() { fallback } else { body };
    source
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Skill directory".to_string())
}

/// Sort the skills vector in place according to the resolved page.
fn sort_skills(
    skills: &mut [(SkillInfo, std::time::SystemTime)],
    resolved: &ResolvedPage,
) {
    let field = resolved.sort_field.as_deref().unwrap_or("name");
    match field {
        "created_at" => {
            skills.sort_by_key(|a| a.1);
            if resolved.descending {
                skills.reverse();
            }
        }
        _ => {
            skills.sort_by(|a, b| a.0.name.cmp(&b.0.name));
            if resolved.descending {
                skills.reverse();
            }
        }
    }
}

/// GET /api/skills/:id - Get a single skill.
pub async fn get_skill(
    State(state): State<Arc<AppState>>,
    AppPath(name): AppPath<String>,
) -> Result<Json<SkillDetail>, AppError> {
    validate_resource_name(&name)?;
    let skill_dir = state
        .workspace_root
        .join(".agents")
        .join("skills")
        .join(&name);
    let skill_path = skill_dir.join("SKILL.md");

    if !skill_path.exists() {
        return Err(AppError::from(Error::not_found(format!(
            "skill '{name}'"
        ))));
    }

    // The detail endpoint benefits from the same mtime-based
    // caching as the list endpoint: SKILL.md is read once and
    // frontmatter + body stay cached until either the file is
    // edited (mtime bump → re-read) or `create_skill` /
    // `delete_skill` / `reload_skills` clears the cache. Without
    // this cache, every detail-page mount re-reads the file and
    // re-parses the YAML frontmatter.
    let cached = cached_detail_for(&skill_path, &name).ok_or_else(|| {
        AppError::from(Error::not_found(format!("skill '{name}'")))
    })?;
    Ok(Json(SkillDetail {
        name: cached.name,
        description: cached.description,
        path: skill_path.to_string_lossy().to_string(),
        frontmatter: cached.frontmatter,
        body: cached.body,
    }))
}

/// Request body for `POST /api/v1/skills` (create).
#[derive(Deserialize, validator::Validate)]
pub struct CreateSkillRequest {
    #[validate(length(min = 1, message = "must not be empty"))]
    pub name: String,
    /// SKILL.md body content (full file, with frontmatter).
    #[validate(length(min = 1, message = "must not be empty"))]
    pub content: String,
}

/// Response from `POST /api/v1/skills`.
#[derive(Serialize)]
pub struct CreateSkillResponse {
    pub name: String,
    pub path: String,
}

/// POST /api/v1/skills - Create a new skill from raw SKILL.md content.
///
/// Writes the content to
/// `<workspace>/.agents/skills/<name>/SKILL.md`. The directory is
/// created if missing. Returns `409 Conflict` if the skill
/// already exists. The request body is the raw SKILL.md content
/// (frontmatter + markdown body), matching the on-disk format.
pub async fn create_skill(
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<CreateSkillRequest>,
) -> Result<Json<CreateSkillResponse>, AppError> {
    validate_resource_name(&req.name)?;
    let skill_dir = state
        .workspace_root
        .join(".agents")
        .join("skills")
        .join(&req.name);
    let skill_path = skill_dir.join("SKILL.md");

    // Skip the previous `skill_path.exists()` pre-check and rely
    // on `OpenOptions::create_new` (O_EXCL on POSIX) to atomically
    // refuse the write if the file already exists. That collapses
    // two syscalls (`exists()` + `write()`) into one syscall on
    // both the conflict path and the happy path — and is race-free
    // in a way the previous check-then-write was not.
    std::fs::create_dir_all(&skill_dir).map_err(|e| {
        Error::internal(format!("failed to create skill directory: {e}"))
    })?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&skill_path)
    {
        Ok(mut f) => {
            if let Err(e) = f.write_all(req.content.as_bytes()) {
                return Err(AppError::from(Error::internal(format!(
                    "failed to write SKILL.md: {e}"
                ))));
            }
        }
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            return Err(AppError::from(Error::already_exists(format!(
                "skill '{}'",
                req.name
            ))));
        }
        Err(e) => {
            return Err(AppError::from(Error::internal(format!(
                "failed to write SKILL.md: {e}"
            ))));
        }
    }
    // Drop any stale cache entry for this skill — the new SKILL.md
    // has a strictly-greater mtime so the next read would refresh
    // anyway, but clearing avoids the stat() round trip and also
    // drops any orphan entry for a previous failed create.
    invalidate_list_cache();

    Ok(Json(CreateSkillResponse {
        name: req.name,
        path: skill_path.to_string_lossy().to_string(),
    }))
}

/// DELETE /api/v1/skills/{name} - Remove a skill.
///
/// Deletes `<workspace>/.agents/skills/<name>/` recursively.
/// Returns `404 Not Found` if the skill does not exist.
pub async fn delete_skill(
    State(state): State<Arc<AppState>>,
    AppPath(name): AppPath<String>,
) -> Result<Json<SkillDeleteResponse>, AppError> {
    validate_resource_name(&name)?;
    let skill_dir = state
        .workspace_root
        .join(".agents")
        .join("skills")
        .join(&name);

    // Skip the previous `skill_dir.exists()` pre-check and let
    // `remove_dir_all` surface the NotFound error directly. That
    // collapses two syscalls (`exists()` + `remove_dir_all()`)
    // into one syscall, and maps the kernel's NotFound straight
    // to our 404 — no race window where a concurrent delete
    // could pass the pre-check and then trip the actual op.
    match std::fs::remove_dir_all(&skill_dir) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(AppError::from(Error::not_found(format!(
                "skill '{name}'"
            ))));
        }
        Err(e) => {
            return Err(AppError::from(Error::internal(format!(
                "failed to remove skill directory: {e}"
            ))));
        }
    }
    // The cache is keyed by file presence + mtime; with the
    // directory gone, `cached_list_entry_for` would simply not
    // find it on next read — but invalidating here keeps the
    // process memory footprint bounded on long-running servers
    // that see frequent delete churn.
    invalidate_list_cache();

    Ok(Json(SkillDeleteResponse { name }))
}

/// Response from `POST /api/v1/skills/reload`.
#[derive(Serialize)]
pub struct ReloadSkillsResponse {
    /// Number of skills found in the workspace after reload.
    pub count: usize,
}

/// POST /api/v1/skills/reload - Rescan the skills directory.
///
/// Re-reads `<workspace>/.agents/skills/` from disk so newly
/// added or deleted SKILL.md files are picked up. Returns the
/// count of skill directories found after the rescan so the
/// caller can detect drift between the on-disk state and what
/// the previous list response showed.
pub async fn reload_skills(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ReloadSkillsResponse>, AppError> {
    let skills_dir = state.workspace_root.join(".agents").join("skills");
    let count = if skills_dir.exists() {
        std::fs::read_dir(&skills_dir)
            .map(|entries| entries.flatten().count())
            .unwrap_or(0)
    } else {
        0
    };
    // Explicit reload means the caller wants the next read to
    // re-scan disk; mtime-based invalidation would also fire
    // eventually but the user explicitly asked for a refresh.
    invalidate_list_cache();
    Ok(Json(ReloadSkillsResponse { count }))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::*;

    fn skill(name: &str, mtime: SystemTime) -> (SkillInfo, SystemTime) {
        (
            SkillInfo {
                name: name.to_string(),
                description: String::new(),
            },
            mtime,
        )
    }

    fn resolved(sort_field: Option<&str>, descending: bool) -> ResolvedPage {
        ResolvedPage {
            last_seen_id: None,
            effective_limit: 100,
            sort_field: sort_field.map(String::from),
            descending,
        }
    }

    fn names(skills: &[(SkillInfo, SystemTime)]) -> Vec<&str> {
        skills.iter().map(|(s, _)| s.name.as_str()).collect()
    }

    #[test]
    fn sort_skills_by_name_ascending() {
        let mut skills = vec![
            skill("zelda", UNIX_EPOCH + Duration::from_secs(30)),
            skill("alpha", UNIX_EPOCH + Duration::from_secs(10)),
            skill("mango", UNIX_EPOCH + Duration::from_secs(20)),
        ];
        sort_skills(&mut skills, &resolved(Some("name"), false));
        assert_eq!(names(&skills), vec!["alpha", "mango", "zelda"]);
    }

    #[test]
    fn sort_skills_by_name_descending() {
        let mut skills = vec![
            skill("zelda", UNIX_EPOCH + Duration::from_secs(30)),
            skill("alpha", UNIX_EPOCH + Duration::from_secs(10)),
            skill("mango", UNIX_EPOCH + Duration::from_secs(20)),
        ];
        sort_skills(&mut skills, &resolved(Some("name"), true));
        assert_eq!(names(&skills), vec!["zelda", "mango", "alpha"]);
    }

    #[test]
    fn sort_skills_by_created_at_ascending() {
        let mut skills = vec![
            skill("zelda", UNIX_EPOCH + Duration::from_secs(30)),
            skill("alpha", UNIX_EPOCH + Duration::from_secs(10)),
            skill("mango", UNIX_EPOCH + Duration::from_secs(20)),
        ];
        sort_skills(&mut skills, &resolved(Some("created_at"), false));
        // Order by mtime ascending: alpha(10), mango(20), zelda(30).
        assert_eq!(names(&skills), vec!["alpha", "mango", "zelda"]);
    }

    #[test]
    fn sort_skills_by_created_at_descending() {
        let mut skills = vec![
            skill("zelda", UNIX_EPOCH + Duration::from_secs(30)),
            skill("alpha", UNIX_EPOCH + Duration::from_secs(10)),
            skill("mango", UNIX_EPOCH + Duration::from_secs(20)),
        ];
        sort_skills(&mut skills, &resolved(Some("created_at"), true));
        // Order by mtime descending: zelda(30), mango(20), alpha(10).
        assert_eq!(names(&skills), vec!["zelda", "mango", "alpha"]);
    }

    #[test]
    fn sort_skills_default_field_is_name() {
        // When sort_field is None, the default is "name".
        let mut skills = vec![
            skill("charlie", UNIX_EPOCH + Duration::from_secs(30)),
            skill("alpha", UNIX_EPOCH + Duration::from_secs(10)),
            skill("bravo", UNIX_EPOCH + Duration::from_secs(20)),
        ];
        sort_skills(&mut skills, &resolved(None, false));
        assert_eq!(names(&skills), vec!["alpha", "bravo", "charlie"]);
    }

    #[test]
    fn sort_skills_mixed_name_and_timestamp_order_differ() {
        // Verifies that "name" and "created_at" produce different
        // orders when names and timestamps are inversely related.
        let make = || {
            vec![
                skill("c", UNIX_EPOCH + Duration::from_secs(10)),
                skill("b", UNIX_EPOCH + Duration::from_secs(20)),
                skill("a", UNIX_EPOCH + Duration::from_secs(30)),
            ]
        };

        let mut by_name = make();
        sort_skills(&mut by_name, &resolved(Some("name"), false));
        assert_eq!(names(&by_name), vec!["a", "b", "c"]);

        let mut by_time = make();
        sort_skills(&mut by_time, &resolved(Some("created_at"), false));
        assert_eq!(names(&by_time), vec!["c", "b", "a"]);
    }

    #[test]
    fn sort_skills_empty_input_is_noop() {
        let mut skills: Vec<(SkillInfo, SystemTime)> = Vec::new();
        sort_skills(&mut skills, &resolved(Some("name"), false));
        assert!(skills.is_empty());
    }

    #[test]
    fn parse_skill_md_extracts_frontmatter_and_body() {
        let content = "---\nname: foo\ndescription: A foo skill\nmetadata:\n  category: tooling\n---\n# Foo\n\nBody paragraph.\n";
        let (fm, body) = parse_skill_md(content);
        assert_eq!(fm.get("name").unwrap(), &serde_json::json!("foo"));
        assert_eq!(
            fm.get("description").unwrap(),
            &serde_json::json!("A foo skill")
        );
        assert_eq!(
            fm.get("metadata").and_then(|v| v.get("category")).unwrap(),
            &serde_json::json!("tooling")
        );
        assert!(body.starts_with("# Foo"));
    }

    #[test]
    fn parse_skill_md_missing_delimiters_returns_full_content_as_body() {
        let content = "# Just a heading\n\nNo frontmatter here.\n";
        let (fm, body) = parse_skill_md(content);
        assert!(fm.is_empty());
        assert_eq!(body, content);
    }

    #[test]
    fn extract_description_picks_first_non_empty_line() {
        let content = "---\nname: foo\n---\n\n# Heading\n\nbody\n";
        // Mirrors the production hot path: extract_body first,
        // then extract_description_from — no YAML parse needed
        // for the list endpoint's short description.
        assert_eq!(
            extract_description_from(extract_body(content), content),
            "# Heading"
        );
    }

    #[test]
    fn extract_description_falls_back_when_body_is_empty() {
        let content = "---\nname: foo\n---\n";
        // Empty body → fall back to first non-empty line of the
        // raw file (the `name:` line in this case).
        let d = extract_description_from(extract_body(content), content);
        assert!(!d.is_empty());
    }
}
