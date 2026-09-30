//! Cache-stable runtime-context snapshots.
//!
//! ## Why a snapshot seam?
//!
//! The system prompt must stay byte-stable across a session so
//! provider prompt caches hit every turn (Anthropic `cache_control`,
//! OpenAI automatic caching). Per-dispatch volatile facts —
//! cwd, platform, today's date, model id — would invalidate the
//! prefix cache on every turn, so they cannot live in the
//! system message.
//!
//! Instead, those facts are surfaced as a **user-role snapshot
//! message** appended right before the next LLM sample. The
//! snapshot is wrapped in the dsh-style frame
//! `"Current runtime context. This snapshot supersedes earlier
//! runtime-context snapshots."` so the model reads it as a
//! durable state declaration that overwrites any earlier
//! snapshot, not as a one-off user turn.
//!
//! ## Why "only when changed"?
//!
//! Most turns have identical facts (the model never sees
//! `Today` advance within a chat session in any realistic
//! scenario). Appending the snapshot every turn would burn
//! tokens for no information gain and would also push the
//! retained history forward — context-compaction logic would
//! eventually have to evict real history to make room for a
//! duplicate snapshot. The loop tracks the last-rendered
//! snapshot text and skips the append when nothing changed:
//! zero token cost, prefix cache untouched.
//!
//! ## Why inject the clock?
//!
//! Determinism in tests. `RuntimeContext::from_runtime` accepts
//! a `chrono::DateTime<Utc>` so the suite can pin `today` to a
//! fixed value across two simulated turns. The production loop
//! reads that instant from the injected
//! [`synthia_core::Clock`](crate::agent::ReActAgent::with_clock), so
//! no production path calls `chrono::Utc::now()` itself.
//!
//! ## Public surface (1 type, 3 methods)
//!
//! - [`RuntimeContext`] — the snapshot data.
//! - [`RuntimeContext::from_runtime`] — build from `std::env`
//!   facts + an injected clock.
//! - [`RuntimeContext::render_snapshot`] — render the
//!   frame-prefixed body (full user-message text).

use chrono::{DateTime, Utc};

/// Prefix prepended to every snapshot body. dsh parity
/// (`packages/core/system-prompt/src/index.ts::joinContextSections`):
/// the model reads the frame as a *supersession marker*, not a
/// one-off user turn. Changing the wording is a breaking
/// prompt-cache contract; if a future revision ever wants to
/// drop or rephrase it, every prior cached prefix becomes
/// stale.
pub const SNAPSHOT_FRAME: &str = "Current runtime context. This snapshot supersedes \
     earlier runtime-context snapshots.";

/// Per-dispatch runtime facts rendered as a user-role snapshot
/// message. Cheap to clone (six `String`s + `Option<bool>` +
/// `Option<String>`) so the loop builds one fresh on every
/// iteration without measurable cost.
///
/// `is_git_repo` is `Option<bool>` rather than `bool` so a
/// caller that doesn't know (the canonical ReActAgent path has
/// no git dep wired in) can render `unknown` instead of
/// guessing.
#[derive(Clone, Debug)]
pub struct RuntimeContext {
    /// Current working directory of the agent run.
    pub cwd: String,
    /// Workspace root handed to built-in tools via
    /// `synthia_tool::Context`. Often equal to `cwd`, but
    /// separated so a delegated sub-agent that pins to a project
    /// root while the parent loops in a scratch dir still tells
    /// the model where files live.
    pub worktree: String,
    /// `true` if `worktree` is inside a git working tree.
    /// `None` means "the runtime didn't check" and renders as
    /// `unknown` rather than `false`.
    pub is_git_repo: Option<bool>,
    /// `std::env::consts::OS` value (`"linux"`, `"macos"`,
    /// `"windows"`, …). Pre-stringified so the struct is
    /// plain-old-data and `Clone` stays trivial.
    pub platform: String,
    /// Human-readable date string (`Mon Jan 1 2026`). Built from
    /// the *injected* `now` so two turns in the same minute
    /// render byte-identical bodies and the loop can short-circuit
    /// the append. Format is `chrono`'s `Local`-style: weekday +
    /// month abbrev + day-of-month + year.
    pub today: String,
    /// Active model identifier (provider + model). Surfaced
    /// verbatim so the model can self-identify when asked.
    /// `None` means "not yet resolved" — renders as `unknown` in
    /// the body.
    pub model_id: Option<String>,
}

impl RuntimeContext {
    /// Build a [`RuntimeContext`] from the loop's
    /// `workspace_root`, `std::env` facts, and an injected
    /// `now` clock.
    ///
    /// `is_git_repo` defaults to `None` because synthia-harness
    /// has no git dependency — callers that want a definitive
    /// value can construct via
    /// `RuntimeContext { is_git_repo: Some(detect()), .. }`.
    pub fn from_runtime(workspace_root: &str, now: DateTime<Utc>) -> Self {
        Self {
            cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| String::from("unknown")),
            worktree: workspace_root.to_string(),
            is_git_repo: None,
            platform: std::env::consts::OS.to_string(),
            today: now.format("%a %b %-d %Y").to_string(),
            model_id: None,
        }
    }

    /// Render the user-role snapshot body (frame + key/value
    /// lines). Pure: no I/O, no `Utc::now()` reads. Two turns
    /// with identical facts render byte-identical strings, so
    /// the loop can short-circuit the append.
    pub fn render_snapshot(&self) -> String {
        let git = match self.is_git_repo {
            Some(true) => "yes",
            Some(false) => "no",
            None => "unknown",
        };
        let model = self.model_id.as_deref().unwrap_or("unknown");
        format!(
            "{frame}\n\n  Working directory: {cwd}\n  \
             Workspace root: {worktree}\n  \
             Is directory a git repo: {git}\n  \
             Platform: {platform}\n  \
             Today: {today}\n  \
             Model: {model}",
            frame = SNAPSHOT_FRAME,
            cwd = self.cwd,
            worktree = self.worktree,
            git = git,
            platform = self.platform,
            today = self.today,
            model = model,
        )
    }
}
