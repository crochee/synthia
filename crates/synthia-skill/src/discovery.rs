//! Skill discovery — walk the workspace + user home for
//! `SKILL.md` files.
//!
//! Mirrors `opencode/packages/opencode/src/skill/index.ts`:
//!
//! 1. **Project skills** at `<workspace>/.agents/skills/**/SKILL.md`
//! 2. **User skills** at `~/.claude/skills/**/SKILL.md`
//!    and `~/.agents/skills/**/SKILL.md`
//!
//! The project root wins over the user root when two skills
//! share a name (matches the opencode precedence: project
//! overrides personal). A user-skill that fails to parse is
//! silently dropped — same leniency opencode / Grok Build
//! apply to broken third-party skills so a malformed file
//! never poisons the whole registry.
//!
//! Discovery is cheap (`read_dir` + an existence check), and
//! is invoked by:
//!
//! - `AppState::new` once at server startup, to seed the
//!   `<available_skills>` prompt block.
//! - `POST /api/v1/skills/reload` on demand.
//!
//! The `Skill` value type is the canonical surface — callers
//! see only `name`, `description`, `location`, `content`.

use std::path::{Path, PathBuf};

use crate::skill::Skill;

/// Project-level skills root (relative to the workspace root).
///
/// Matches Anthropic's `<workspace>/.claude/skills/` and
/// OpenCode's `<workspace>/.agents/skills/` conventions;
/// Synthia uses `.agents/` so the directory is shared with
/// other agent definitions (subagents, hooks, etc).
pub const PROJECT_SKILLS_DIR: &str = ".agents/skills";

/// User-level skills roots, scanned in this order.
///
/// Mirrors `opencode/packages/opencode/src/skill/index.ts`'s
/// `EXTERNAL_SKILL_PATTERN` set: `.claude/skills/` first
/// (Anthropic Agent Skills convention), then `.agents/skills/`
/// (OpenCode / Codex convention). The first match wins; later
/// duplicates are dropped silently.
const USER_SKILLS_DIRS: &[&str] = &[".claude/skills", ".agents/skills"];

/// Discover all skills visible to the given workspace.
///
/// `workspace_root` is the project root the agent runs in;
/// user-level skills are read from `$HOME/{.claude,.agents}/skills`.
/// Project skills take precedence over user skills when two
/// files share a name.
///
/// Returned order: project skills first (in `read_dir` order),
/// then user skills. The caller is free to sort before
/// rendering the `<available_skills>` block.
pub fn discover_skills(workspace_root: &Path) -> Vec<Skill> {
    let mut seen: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut out: Vec<Skill> = Vec::new();

    for path in walk_skill_dirs(&workspace_root.join(PROJECT_SKILLS_DIR)) {
        match Skill::from_path(&path) {
            Ok(skill) => {
                if seen.insert(skill.name.clone()) {
                    out.push(skill);
                }
            }
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "skipping malformed SKILL.md during project discovery"
                );
            }
        }
    }

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for rel in USER_SKILLS_DIRS {
            for path in walk_skill_dirs(&home.join(rel)) {
                match Skill::from_path(&path) {
                    Ok(skill) => {
                        if seen.insert(skill.name.clone()) {
                            out.push(skill);
                        }
                    }
                    Err(err) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %err,
                            "skipping malformed SKILL.md during user discovery"
                        );
                    }
                }
            }
        }
    }

    out
}

/// Walk a `skills/` directory recursively, returning every
/// `SKILL.md` it finds.
///
/// Bounded to a depth of 5 (matches grok-build's
/// `MAX_SKILL_WALK_DEPTH`) so a runaway symlink loop cannot
/// pin a server thread.
fn walk_skill_dirs(root: &Path) -> Vec<PathBuf> {
    const MAX_DEPTH: usize = 5;
    let mut out: Vec<PathBuf> = Vec::new();
    if !root.is_dir() {
        return out;
    }
    walk_recursive(root, 0, MAX_DEPTH, &mut out);
    out.sort();
    out
}

fn walk_recursive(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<PathBuf>,
) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let skill_md = path.join("SKILL.md");
            if skill_md.is_file() {
                out.push(skill_md);
            }
            walk_recursive(&path, depth + 1, max_depth, out);
        }
    }
}

/// Best-effort fallback for `HOME` when `$HOME` is unset —
/// reserved hook for systems that don't expose `HOME`. Synthia
/// targets POSIX + Windows; both always carry `HOME`/`USERPROFILE`,
/// so this is currently a no-op kept for forward-compat with
/// container runtimes that strip the env.
#[allow(dead_code)]
fn home_dir_fallback() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Serialize tests that mutate `$HOME` — the env var is
    /// process-global and parallel tests would step on each
    /// other.
    static HOME_LOCK: Mutex<()> = Mutex::new(());

    /// RAII helper: pin `$HOME` to a tempdir for the lifetime
    /// of the guard so tests don't pick up the developer's
    /// installed `.claude/skills/` set.
    struct ScopedHome {
        previous: Option<std::ffi::OsString>,
        dir: tempfile::TempDir,
    }
    impl ScopedHome {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let previous = std::env::var_os("HOME");
            unsafe {
                std::env::set_var("HOME", dir.path());
            }
            Self { previous, dir }
        }
    }
    impl Drop for ScopedHome {
        fn drop(&mut self) {
            // `set_var` / `remove_var` are `unsafe` in Rust
            // 2024 edition. Test-only env mutation is safe
            // (single-threaded `Drop`, no concurrent reads),
            // so the `unsafe` block is here purely to satisfy
            // the compiler.
            unsafe {
                match self.previous.take() {
                    Some(v) => std::env::set_var("HOME", v),
                    None => {
                        std::env::remove_var("HOME");
                    }
                }
            }
        }
    }

    fn write_skill(dir: &Path, name: &str, body: &str) -> PathBuf {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let skill_md = skill_dir.join("SKILL.md");
        std::fs::write(&skill_md, body).unwrap();
        skill_md
    }

    /// Run a closure with `$HOME` pinned to a tempdir.
    /// Returns whatever the closure returns. RAII restores
    /// `$HOME` on scope exit (panic or success).
    fn with_scoped_home<F, R>(f: F) -> R
    where
        F: FnOnce(&tempfile::TempDir) -> R,
    {
        // `unwrap_or_else(|e| e.into_inner())` ignores a
        // poisoned mutex: if a sibling test panicked while
        // holding the lock, we still want subsequent tests
        // to run (the env restoration is idempotent).
        let _guard = HOME_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let scoped = ScopedHome::new();
        f(&scoped.dir)
    }

    /// Discovery MUST walk a project `.agents/skills/` tree
    /// recursively and surface every well-formed SKILL.md —
    /// not just the immediate children.
    #[test]
    fn discovers_nested_project_skills() {
        with_scoped_home(|_home| {
            let dir = tempfile::tempdir().unwrap();
            let skills_root = dir.path().join(PROJECT_SKILLS_DIR);
            write_skill(
                &skills_root,
                "code-review",
                "---\nname: code-review\ndescription: Review.\n---\n\nBody.\n",
            );
            let nested_root = skills_root.join("nested");
            write_skill(
                &nested_root,
                "deeper",
                "---\nname: deeper\ndescription: Deep.\n---\n\nDeep body.\n",
            );

            let skills = discover_skills(dir.path());
            let names: Vec<&str> =
                skills.iter().map(|s| s.name.as_str()).collect();
            assert!(names.contains(&"code-review"), "got: {names:?}");
            assert!(names.contains(&"deeper"), "got: {names:?}");
        });
    }

    /// A malformed SKILL.md (missing `---` delimiters) MUST
    /// NOT poison the whole registry — discovery continues
    /// and the broken file is dropped with a warning.
    #[test]
    fn malformed_skills_are_skipped_not_fatal() {
        with_scoped_home(|_home| {
            let dir = tempfile::tempdir().unwrap();
            let skills_root = dir.path().join(PROJECT_SKILLS_DIR);
            write_skill(
                &skills_root,
                "good",
                "---\nname: good\ndescription: Good.\n---\n\nBody.\n",
            );
            write_skill(&skills_root, "bad", "no frontmatter here at all\n");
            let skills = discover_skills(dir.path());
            let names: Vec<&str> =
                skills.iter().map(|s| s.name.as_str()).collect();
            assert_eq!(names, vec!["good"]);
        });
    }

    /// Discovery MUST terminate on runaway nesting (e.g. a
    /// recursive symlink) rather than stack-overflow. Bound
    /// pinned at 5 levels (matches grok-build) so level
    /// index 5 and beyond are dropped.
    #[test]
    fn depth_bound_is_respected() {
        with_scoped_home(|_home| {
            let dir = tempfile::tempdir().unwrap();
            let mut root = dir.path().join(PROJECT_SKILLS_DIR);
            for level in 0..7 {
                root = root.join(format!("level-{level}"));
                write_skill(
                    &root,
                    &format!("deep-{level}"),
                    &format!(
                        "---\nname: deep-{level}\ndescription: Deep {level}.\n---\n\nBody.\n"
                    ),
                );
            }
            let skills = discover_skills(dir.path());
            let names: Vec<&str> =
                skills.iter().map(|s| s.name.as_str()).collect();
            // levels 0..=4 are reachable (5 levels of nesting
            // inside the skills root); level 5+ are dropped
            // by the depth bound.
            for level in 0..=4 {
                let n = format!("deep-{level}");
                assert!(
                    names.contains(&n.as_str()),
                    "{n} missing from {names:?}"
                );
            }
            assert!(
                !names.contains(&"deep-5"),
                "deep-5 should be past the depth bound; got: {names:?}"
            );
        });
    }

    /// User-level skills at `$HOME/.claude/skills/` and
    /// `$HOME/.agents/skills/` MUST be discoverable alongside
    /// project skills (Anthropic / OpenCode / Codex
    /// convention). Project skills win on name collisions.
    #[test]
    fn discovers_user_skills_from_home() {
        with_scoped_home(|home| {
            let dir = tempfile::tempdir().unwrap();

            write_skill(
                &home.path().join(".claude").join("skills"),
                "user-claude",
                "---\nname: user-claude\ndescription: From .claude.\n---\n\nBody.\n",
            );
            write_skill(
                &home.path().join(".agents").join("skills"),
                "user-agents",
                "---\nname: user-agents\ndescription: From .agents.\n---\n\nBody.\n",
            );
            write_skill(
                &dir.path().join(PROJECT_SKILLS_DIR),
                "project",
                "---\nname: project\ndescription: Project.\n---\n\nBody.\n",
            );

            let skills = discover_skills(dir.path());
            let names: Vec<&str> =
                skills.iter().map(|s| s.name.as_str()).collect();
            assert!(names.contains(&"user-claude"), "got: {names:?}");
            assert!(names.contains(&"user-agents"), "got: {names:?}");
            assert!(names.contains(&"project"), "got: {names:?}");
        });
    }
}
