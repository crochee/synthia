//! Tests for the `RuntimeContext` snapshot seam.
//!
//! The snapshot is the per-dispatch volatile-fact carrier
//! (cwd, today, model id, platform, git status). It is
//! rendered as a user-role message in front of every
//! iteration so the system prompt stays byte-stable for
//! prompt caching. The tests below pin:
//!
//! - the dsh-style supersession frame that opens the
//!   snapshot body (any wording change invalidates every
//!   prior cached prefix),
//! - the `is_git_repo: None ⇒ "unknown"` vs
//!   `Some(false) ⇒ "no"` distinction (so the model can
//!   tell "didn't check" from "checked and found no"),
//! - the use of an *injected* clock so two consecutive
//!   `from_runtime` calls with the same `now` produce
//!   byte-identical snapshots — the loop-level invariant
//!   that lets the re-append step short-circuit when
//!   nothing changed.

use super::{super::RuntimeContext, support::fixed_runtime_context};

/// The snapshot body is prefixed with the dsh-style
/// supersession frame and every key/value field renders
/// verbatim below it. The frame text is part of the
/// prompt-cache contract — any wording change
/// invalidates every prior cached prefix.
#[test]
fn runtime_context_renders_snapshot_with_supersession_frame() {
    let ctx = fixed_runtime_context();
    let snapshot = ctx.render_snapshot();

    assert!(
        snapshot.starts_with(
            "Current runtime context. This snapshot supersedes \
             earlier runtime-context snapshots."
        ),
        "snapshot must start with the dsh-style supersession frame; \
         got:\n{snapshot}"
    );
    assert!(snapshot.contains("Working directory: /tmp/cwd"));
    assert!(snapshot.contains("Workspace root: /tmp/worktree"));
    assert!(snapshot.contains("Is directory a git repo: yes"));
    assert!(snapshot.contains("Platform: linux"));
    assert!(snapshot.contains("Today: Mon Jan 1 2026"));
    assert!(snapshot.contains("Model: anthropic/claude-4.6"));
}

/// `is_git_repo: None` renders `unknown` rather than
/// `false` so the model can distinguish "the runtime
/// didn't check" from "checked and found no".
#[test]
fn runtime_context_renders_unknown_when_git_status_missing() {
    let mut ctx = fixed_runtime_context();
    ctx.is_git_repo = None;
    let snapshot = ctx.render_snapshot();
    assert!(
        snapshot.contains("Is directory a git repo: unknown"),
        "missing git status must surface as `unknown`, not `no`; \
         got:\n{snapshot}"
    );
}

/// `is_git_repo: Some(false)` renders `no`.
#[test]
fn runtime_context_renders_no_when_git_status_false() {
    let mut ctx = fixed_runtime_context();
    ctx.is_git_repo = Some(false);
    let snapshot = ctx.render_snapshot();
    assert!(
        snapshot.contains("Is directory a git repo: no"),
        "git=false must surface as `no`; got:\n{snapshot}"
    );
}

/// `RuntimeContext::from_runtime` populates every field
/// without panicking, even when `current_dir()` is
/// unreadable (it falls back to `unknown`). Platform and
/// `today` (built from the injected clock) must be
/// non-empty.
#[test]
fn runtime_context_from_runtime_populates_required_fields() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let ctx = RuntimeContext::from_runtime("/tmp/worktree", now);
    assert_eq!(ctx.worktree, "/tmp/worktree");
    assert!(!ctx.platform.is_empty(), "platform must be non-empty");
    assert!(!ctx.today.is_empty(), "today must be non-empty");
    assert!(
        ctx.is_git_repo.is_none(),
        "from_runtime leaves git status as `None` (no git dep)"
    );
    assert!(
        ctx.model_id.is_none(),
        "from_runtime leaves model_id as `None`"
    );
    // cwd may be `unknown` if the test runner has no cwd;
    // either is fine — the contract is "non-panicking".
    let _ = ctx.cwd;
}

/// `RuntimeContext::from_runtime` uses the *injected*
/// `now` clock for `today`, not `Utc::now()` — that's
/// how a test pins the snapshot text across two turns.
/// Without the injection, two consecutive `from_runtime`
/// calls could span a date boundary and produce
/// different snapshots; the contract pins the
/// determinism.
#[test]
fn runtime_context_from_runtime_uses_injected_clock() {
    let fixed_now =
        chrono::DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
    let ctx = RuntimeContext::from_runtime("/tmp/worktree", fixed_now);
    assert_eq!(ctx.today, "Thu Jan 1 2026");
}

/// Pure-string contract: same `RuntimeContext` ⇒
/// byte-identical snapshot. The loop relies on this to
/// short-circuit the append when facts don't change.
#[test]
fn runtime_context_render_is_pure_and_deterministic() {
    let a = fixed_runtime_context();
    let b = fixed_runtime_context();
    assert_eq!(a.render_snapshot(), b.render_snapshot());
}

/// Two consecutive `from_runtime` calls with the SAME
/// `now` argument produce byte-identical snapshots.
/// This is the loop-level invariant: "unchanged facts
/// across turns append NOTHING".
#[test]
fn runtime_context_with_same_clock_yields_identical_body() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let a = RuntimeContext::from_runtime("/tmp/worktree", now);
    let b = RuntimeContext::from_runtime("/tmp/worktree", now);
    assert_eq!(a.render_snapshot(), b.render_snapshot());
}

/// Two consecutive `from_runtime` calls with DIFFERENT
/// `now` arguments produce DIFFERENT snapshots —
/// verifies that the snapshot change is what triggers
/// the loop's re-append, not a quiet no-op.
#[test]
fn runtime_context_with_different_clock_yields_different_body() {
    let now_a = chrono::DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let now_b = chrono::DateTime::parse_from_rfc3339("2026-01-02T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let a = RuntimeContext::from_runtime("/tmp/worktree", now_a);
    let b = RuntimeContext::from_runtime("/tmp/worktree", now_b);
    assert_ne!(a.render_snapshot(), b.render_snapshot());
}
