//! Git worktree isolation — give each sub-agent its own copy of
//! the repo so its edits cannot trample the parent's working tree.
//!
//! Slice D (R30 — pi-subagents `worktree.ts` parity). When a
//! caller attaches [`WorktreeSpec`] to a
//! [`crate::task::TaskSpec`], the delegation layer:
//!
//! 1. Calls [`create_worktree`] to mint a detached worktree at
//!    `HEAD` under a fresh temp directory. The branch that will
//!    receive any dirty changes is reserved up front as
//!    `synthia/agent-<ulid>`.
//! 2. Runs the child against a [`CommandRunner`] that sees the
//!    worktree path as its working directory.
//! 3. After the child (and the gate, if any) finishes, calls
//!    [`cleanup_worktree`]. A clean worktree is removed; a
//!    dirty worktree gets its changes committed on the
//!    reserved branch and the cleanup result reports the new
//!    branch name + path so the parent can surface them to the
//!    user.
//!
//! # Runtime neutrality
//!
//! Every `git` invocation goes through the same
//! [`CommandRunner`] trait the gate uses — production runs use
//! [`StdCommandRunner`](crate::gate::StdCommandRunner), tests use a recording fake. The trait
//! is `Send + Sync` so the worktree, gate, and child-tool layers
//! can share a single runner without further plumbing.
//!
//! # Branch naming
//!
//! `synthia/agent-<ulid>` mirrors the pi-subagents
//! `pi-agent-<agentId>` convention but anchors the prefix to
//! `synthia/` so multiple agent families can coexist in one
//! repo without colliding. ULID is sortable and unique enough
//! that a fresh value per spawn avoids the "branch already
//! exists" retry dance the TS code pays on every commit.

use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;
use ulid::Ulid;

#[cfg(test)]
use crate::gate::CommandOutcome;
use crate::gate::{CommandRunner, CommandStopReason};

/// Marker the parent sets on a [`crate::task::TaskSpec`]
/// to ask for git-worktree isolation.
///
/// Presence alone is the contract: there are no fields because
/// the worktree is fully auto-generated (temp dir + ULID
/// branch). The type exists to make the `isolation` field on
/// [`crate::task::TaskSpec`] self-documenting at
/// the schema level — a plain `bool` would force callers to
/// re-read the contract to know that isolation also reserves
/// the branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WorktreeSpec;

/// Returned by [`create_worktree`]: the worktree is detached,
/// pointing at the same `HEAD` as the base repo, but reserved
/// under `branch` so a later commit can land cleanly.
///
/// `path` is the temp dir the worktree was created in (the
/// copied repo's root). `base_sha` is the SHA the worktree was
/// detached from; cleanup uses it as the "no-op baseline" for
/// deciding whether the worktree is dirty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeInfo {
    /// Absolute path to the worktree directory (the copied
    /// repo's root).
    pub path: PathBuf,
    /// Branch name reserved for this worktree. Created at
    /// cleanup time iff the worktree is dirty.
    pub branch: String,
    /// Commit SHA that the worktree was created from.
    pub base_sha: String,
}

/// Outcome of [`cleanup_worktree`].
///
/// The parent surfaces this to the user when `has_changes` is
/// `true`: a delegated sub-agent that produced a branch the
/// caller can inspect / merge / discard. When `has_changes` is
/// `false` the worktree was removed silently — nothing for the
/// caller to act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeCleanupResult {
    /// Whether the worktree had uncommitted changes at
    /// cleanup time.
    pub has_changes: bool,
    /// Branch name where the dirty worktree was committed
    /// (only set when `has_changes` is true).
    pub branch: Option<String>,
    /// Worktree path; the branch lives in the **base repo** so
    /// `path` is informational (the temp dir is gone after
    /// cleanup).
    pub path: Option<String>,
    /// SHA the worktree was created from; echoed for the caller's
    /// log / structured-event payload.
    pub base_sha: String,
}

/// Errors raised when the worktree lifecycle cannot complete
/// against the supplied [`CommandRunner`]. Spawn / non-zero
/// exit failures are surfaced as a single variant so the
/// caller can render them in a tool result without having to
/// pattern-match.
#[derive(Debug, Error)]
pub enum WorktreeError {
    /// `git rev-parse` could not locate HEAD — the base cwd is
    /// not a git repo or has no commits.
    #[error("worktree base `{cwd}` is not a git repo with commits")]
    NotAGitRepo {
        /// The base directory the parent tried to use.
        cwd: String,
    },
    /// `git worktree add` failed.
    #[error("git worktree add failed in `{cwd}`: {stderr}")]
    CreateFailed {
        /// The base directory the worktree was attempted in.
        cwd: String,
        /// Captured stderr from `git worktree add`.
        stderr: String,
    },
    /// `git status --porcelain` returned a non-zero status.
    #[error("git status failed in worktree `{path}`: {stderr}")]
    StatusFailed {
        /// The worktree path that failed.
        path: String,
        /// Captured stderr from `git status`.
        stderr: String,
    },
    /// `git add -A` returned a non-zero status.
    #[error("git add failed in worktree `{path}`: {stderr}")]
    AddFailed {
        /// The worktree path that failed.
        path: String,
        /// Captured stderr from `git add`.
        stderr: String,
    },
    /// `git commit` returned a non-zero status (typically a
    /// pre-commit hook failure).
    #[error("git commit failed in worktree `{path}`: {stderr}")]
    CommitFailed {
        /// The worktree path that failed.
        path: String,
        /// Captured stderr from `git commit`.
        stderr: String,
    },
    /// `git branch` returned a non-zero status.
    #[error("git branch failed in worktree `{path}`: {stderr}")]
    BranchFailed {
        /// The worktree path that failed.
        path: String,
        /// Captured stderr from `git branch`.
        stderr: String,
    },
    /// `git worktree remove` returned a non-zero status.
    #[error("git worktree remove failed for `{path}`: {stderr}")]
    RemoveFailed {
        /// The worktree path the runner tried to remove.
        path: String,
        /// Captured stderr from `git worktree remove`.
        stderr: String,
    },
}

/// Create a fresh detached worktree off the base cwd's HEAD.
///
/// `base_cwd` is the directory the parent session was launched
/// in; the worktree is a copy of that repo. `runner` issues the
/// `git` calls so tests can swap in a fake. `cancel` is
/// propagated to the runner but the worktree creation itself is
/// not cancellable mid-flight (one `git worktree add` call).
pub fn create_worktree(
    base_cwd: &Path,
    runner: &dyn CommandRunner,
) -> Result<WorktreeInfo, WorktreeError> {
    // 1. Verify base cwd is a git repo and capture HEAD.
    let head_outcome = runner.run(
        &["git".into(), "rev-parse".into(), "HEAD".into()],
        base_cwd,
        crate::gate::no_cancel_token(),
    );
    let base_sha = match head_outcome.reason {
        CommandStopReason::Exited if head_outcome.status.success() => {
            head_outcome.stdout.trim().to_string()
        }
        _ => {
            return Err(WorktreeError::NotAGitRepo {
                cwd: base_cwd.display().to_string(),
            });
        }
    };

    // 2. Reserve the branch and the temp directory.
    let id = Ulid::generate().to_string();
    let branch = format!("synthia/agent-{id}");
    let worktree_path =
        std::env::temp_dir().join(format!("synthia-harness-{id}"));

    // 3. Detached worktree at HEAD.
    let add_outcome = runner.run(
        &[
            "git".into(),
            "worktree".into(),
            "add".into(),
            "--detach".into(),
            worktree_path.display().to_string(),
            "HEAD".into(),
        ],
        base_cwd,
        crate::gate::no_cancel_token(),
    );
    match add_outcome.reason {
        CommandStopReason::Exited if add_outcome.status.success() => {}
        _ => {
            return Err(WorktreeError::CreateFailed {
                cwd: base_cwd.display().to_string(),
                stderr: add_outcome.stderr,
            });
        }
    }

    Ok(WorktreeInfo {
        path: worktree_path,
        branch,
        base_sha,
    })
}

/// Clean up a worktree after a sub-agent finishes.
///
/// Algorithm (mirrors `cleanupWorktree` in
/// `pi-subagents/src/worktree.ts`):
///
/// 1. `git status --porcelain` — empty ⇒ clean tree.
/// 2. Clean tree, HEAD == `base_sha` ⇒ remove the worktree
///    silently (no changes, nothing to keep).
/// 3. Clean tree, HEAD diverged from `base_sha` ⇒ the agent
///    made committed changes without leaving uncommitted ones
///    (rare; happens if the agent committed itself). Treat as
///    "has changes" and branch from current HEAD.
/// 4. Dirty tree ⇒ `git add -A` + `git commit -m '…'` then
///    `git branch <branch>`. The branch is created in the
///    **base repo** because the worktree's detached HEAD
///    would not otherwise persist. After the branch lands,
///    the worktree is removed so the temp dir does not leak.
///
/// `description` is the agent description (truncated to 200
/// chars) — it ends up in the commit message so the user can
/// tell which sub-agent produced the branch.
pub fn cleanup_worktree(
    base_cwd: &Path,
    info: &WorktreeInfo,
    description: &str,
    runner: &dyn CommandRunner,
) -> WorktreeCleanupResult {
    let safe_desc = description.chars().take(200).collect::<String>();
    let commit_msg = format!("synthia-harness: {safe_desc}");

    // 1. Detect dirt.
    let status_outcome = runner.run(
        &["git".into(), "status".into(), "--porcelain".into()],
        &info.path,
        crate::gate::no_cancel_token(),
    );
    let porcelain = match status_outcome.reason {
        CommandStopReason::Exited if status_outcome.status.success() => {
            status_outcome.stdout
        }
        _ => {
            return WorktreeCleanupResult {
                has_changes: false,
                branch: None,
                path: None,
                base_sha: info.base_sha.clone(),
            };
        }
    };

    let mut has_changes = !porcelain.trim().is_empty();

    if !has_changes {
        // 2/3. No uncommitted changes — check HEAD vs base_sha.
        let head_outcome = runner.run(
            &["git".into(), "rev-parse".into(), "HEAD".into()],
            &info.path,
            crate::gate::no_cancel_token(),
        );
        let current_sha = match head_outcome.reason {
            CommandStopReason::Exited if head_outcome.status.success() => {
                head_outcome.stdout.trim().to_string()
            }
            _ => String::new(),
        };
        if !current_sha.is_empty() && current_sha != info.base_sha {
            has_changes = true;
        }
    }

    let mut created_branch: Option<String> = None;
    if has_changes {
        // Stage + commit on the detached HEAD.
        let add_outcome = runner.run(
            &["git".into(), "add".into(), "-A".into()],
            &info.path,
            crate::gate::no_cancel_token(),
        );
        if !matches!(
            add_outcome.reason,
            CommandStopReason::Exited if add_outcome.status.success()
        ) {
            // Best-effort: even if staging fails, try to remove
            // the worktree and report no branch.
            let _ = runner.run(
                &[
                    "git".into(),
                    "worktree".into(),
                    "remove".into(),
                    "--force".into(),
                    info.path.display().to_string(),
                ],
                base_cwd,
                crate::gate::no_cancel_token(),
            );
            return WorktreeCleanupResult {
                has_changes: false,
                branch: None,
                path: None,
                base_sha: info.base_sha.clone(),
            };
        }

        let commit_outcome = runner.run(
            &[
                "git".into(),
                "commit".into(),
                "--no-verify".into(),
                "-m".into(),
                commit_msg,
            ],
            &info.path,
            crate::gate::no_cancel_token(),
        );
        if !matches!(
            commit_outcome.reason,
            CommandStopReason::Exited if commit_outcome.status.success()
        ) {
            let _ = runner.run(
                &[
                    "git".into(),
                    "worktree".into(),
                    "remove".into(),
                    "--force".into(),
                    info.path.display().to_string(),
                ],
                base_cwd,
                crate::gate::no_cancel_token(),
            );
            return WorktreeCleanupResult {
                has_changes: false,
                branch: None,
                path: None,
                base_sha: info.base_sha.clone(),
            };
        }

        // Create the branch in the base repo pointing at the
        let branch_outcome = runner.run(
            &["git".into(), "branch".into(), info.branch.clone()],
            &info.path,
            crate::gate::no_cancel_token(),
        );
        let branch_succeeded = matches!(
            branch_outcome.reason,
            CommandStopReason::Exited if branch_outcome.status.success()
        );
        if !branch_succeeded {
            // fall back to a unique suffix so we never
            let mut unique = info.branch.clone();
            unique.push('-');
            unique.push_str(&Ulid::generate().to_string());
            let retry = runner.run(
                &["git".into(), "branch".into(), unique.clone()],
                &info.path,
                crate::gate::no_cancel_token(),
            );
            if matches!(
                retry.reason,
                CommandStopReason::Exited if retry.status.success()
            ) {
                created_branch = Some(unique);
            } else {
                let _ = runner.run(
                    &[
                        "git".into(),
                        "worktree".into(),
                        "remove".into(),
                        "--force".into(),
                        info.path.display().to_string(),
                    ],
                    base_cwd,
                    crate::gate::no_cancel_token(),
                );
                return WorktreeCleanupResult {
                    has_changes: false,
                    branch: None,
                    path: None,
                    base_sha: info.base_sha.clone(),
                };
            }
        } else {
            created_branch = Some(info.branch.clone());
        }
    }

    // Remove the worktree regardless of has_changes (the branch
    // is in the base repo, the worktree is just a temp dir).
    let _ = runner.run(
        &[
            "git".into(),
            "worktree".into(),
            "remove".into(),
            "--force".into(),
            info.path.display().to_string(),
        ],
        base_cwd,
        crate::gate::no_cancel_token(),
    );

    WorktreeCleanupResult {
        has_changes,
        branch: if has_changes { created_branch } else { None },
        path: Some(info.path.display().to_string()),
        base_sha: info.base_sha.clone(),
    }
}

/// Parse a `Value` shape `{ "worktree": <spec> }` into a
/// [`WorktreeSpec`]. Used by
/// [`crate::task::TaskSpec::from_value`] to wire
/// the JSON schema into the TaskSpec.
pub(crate) fn parse_worktree_field(
    value: Option<&Value>,
) -> Option<WorktreeSpec> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    Some(WorktreeSpec)
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use std::{
        collections::VecDeque,
        path::PathBuf,
        process::ExitStatus,
        sync::{Arc, Mutex},
    };

    use synthia_core::CancelToken;

    use super::*;
    /// Scripted runner: each `run` removes the next canned
    /// outcome in FIFO order. Tests push outcomes in the order
    /// they expect `git` to be invoked.
    #[derive(Default)]
    struct ScriptedRunner {
        recorded: Mutex<Vec<Vec<String>>>,
        outcomes: Mutex<VecDeque<CommandOutcome>>,
    }

    impl ScriptedRunner {
        fn with_outcomes(outcomes: Vec<CommandOutcome>) -> Self {
            let deque: VecDeque<CommandOutcome> = outcomes.into();
            Self {
                recorded: Mutex::new(Vec::new()),
                outcomes: Mutex::new(deque),
            }
        }

        fn recorded(&self) -> Vec<Vec<String>> {
            self.recorded.lock().expect("lock").clone()
        }
    }

    impl CommandRunner for ScriptedRunner {
        fn run(
            &self,
            argv: &[String],
            _cwd: &Path,
            _cancel: Arc<dyn CancelToken>,
        ) -> CommandOutcome {
            self.recorded.lock().expect("lock").push(argv.to_vec());
            self.outcomes
                .lock()
                .expect("lock")
                .pop_front()
                .unwrap_or_else(|| CommandOutcome {
                    status: synthetic_failure(),
                    stdout: String::new(),
                    stderr: String::new(),
                    reason: CommandStopReason::Exited,
                })
        }
    }
    #[cfg(unix)]
    fn success_status() -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(0)
    }
    #[cfg(not(unix))]
    fn success_status() -> ExitStatus {
        ExitStatus::default()
    }
    #[cfg(unix)]
    fn failure_status() -> ExitStatus {
        // WEXITED with exit code 128: shift so WIFEXITED is true.
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(128 << 8)
    }
    #[cfg(not(unix))]
    fn failure_status() -> ExitStatus {
        ExitStatus::default()
    }
    fn synthetic_failure() -> ExitStatus {
        failure_status()
    }

    fn no_cancel() -> Arc<dyn CancelToken> {
        crate::gate::no_cancel_token()
    }

    fn exit_outcome(
        status: ExitStatus,
        stdout: &str,
        stderr: &str,
    ) -> CommandOutcome {
        CommandOutcome {
            status,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            reason: CommandStopReason::Exited,
        }
    }

    fn temp_base() -> PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn create_worktree_reports_branch_path_and_base_sha() {
        let runner = ScriptedRunner::with_outcomes(vec![
            exit_outcome(success_status(), "abc123\n", ""),
            exit_outcome(success_status(), "", ""),
        ]);
        let info = create_worktree(&temp_base(), &runner).expect("create");
        assert_eq!(info.base_sha, "abc123");
        assert!(info.branch.starts_with("synthia/agent-"));
        assert!(
            info.path.exists()
                || info.path.to_string_lossy().contains("synthia-harness-")
        );
        // Recorded argv order: rev-parse HEAD, worktree add …
        let recorded = runner.recorded();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0][0], "git");
        assert_eq!(recorded[0][1], "rev-parse");
        assert_eq!(recorded[1][1], "worktree");
        assert_eq!(recorded[1][2], "add");
        assert_eq!(recorded[1][3], "--detach");
    }

    #[test]
    fn create_worktree_fails_when_base_is_not_a_repo() {
        let runner = ScriptedRunner::with_outcomes(vec![exit_outcome(
            failure_status(),
            "",
            "fatal: not a git repository",
        )]);
        let err = create_worktree(&temp_base(), &runner).expect_err("err");
        assert!(matches!(err, WorktreeError::NotAGitRepo { .. }));
    }

    #[test]
    fn cleanup_clean_tree_returns_no_branch() {
        let runner = ScriptedRunner::with_outcomes(vec![
            // git status --porcelain
            exit_outcome(success_status(), "", ""),
            // git rev-parse HEAD (matches base_sha)
            exit_outcome(success_status(), "abc123\n", ""),
            // git worktree remove --force …
            exit_outcome(success_status(), "", ""),
        ]);
        let info = WorktreeInfo {
            path: PathBuf::from("/tmp/synthia-harness-fake"),
            branch: "synthia/agent-1".to_string(),
            base_sha: "abc123".to_string(),
        };
        let result = cleanup_worktree(&temp_base(), &info, "desc", &runner);
        assert!(!result.has_changes);
        assert!(result.branch.is_none());
        assert_eq!(result.base_sha, "abc123");
        let recorded = runner.recorded();
        assert_eq!(recorded[0][1], "status");
        assert_eq!(recorded[1][1], "rev-parse");
        assert_eq!(recorded[2][1], "worktree");
        assert_eq!(recorded[2][2], "remove");
    }

    #[test]
    fn cleanup_dirty_tree_commits_and_branches() {
        let runner = ScriptedRunner::with_outcomes(vec![
            // git status --porcelain (dirty)
            exit_outcome(success_status(), " M foo\n", ""),
            // git add -A
            exit_outcome(success_status(), "", ""),
            // git commit --no-verify -m …
            exit_outcome(success_status(), "", ""),
            // git branch synthia/agent-…
            exit_outcome(success_status(), "", ""),
            // git worktree remove --force …
            exit_outcome(success_status(), "", ""),
        ]);
        let info = WorktreeInfo {
            path: PathBuf::from("/tmp/synthia-harness-fake"),
            branch: "synthia/agent-2".to_string(),
            base_sha: "abc123".to_string(),
        };
        let result = cleanup_worktree(&temp_base(), &info, "desc", &runner);
        assert!(result.has_changes);
        assert_eq!(result.branch.as_deref(), Some("synthia/agent-2"));
        let recorded = runner.recorded();
        assert_eq!(recorded[0][1], "status");
        assert_eq!(recorded[1][1], "add");
        assert_eq!(recorded[2][1], "commit");
        assert!(recorded[2].iter().any(|s| s == "-m"));
        assert_eq!(recorded[3][1], "branch");
        assert_eq!(recorded[3][2], "synthia/agent-2");
        assert_eq!(recorded[4][1], "worktree");
        assert_eq!(recorded[4][2], "remove");
    }

    #[test]
    fn cleanup_clean_but_diverged_head_counts_as_changes() {
        let runner = ScriptedRunner::with_outcomes(vec![
            exit_outcome(success_status(), "", ""),
            exit_outcome(success_status(), "def456\n", ""),
            exit_outcome(success_status(), "", ""),
            exit_outcome(success_status(), "", ""),
            exit_outcome(success_status(), "", ""),
        ]);
        let info = WorktreeInfo {
            path: PathBuf::from("/tmp/synthia-harness-fake"),
            branch: "synthia/agent-3".to_string(),
            base_sha: "abc123".to_string(),
        };
        let result = cleanup_worktree(&temp_base(), &info, "desc", &runner);
        assert!(result.has_changes);
        assert_eq!(result.branch.as_deref(), Some("synthia/agent-3"));
    }

    #[test]
    fn cleanup_truncates_long_descriptions() {
        let runner = ScriptedRunner::with_outcomes(vec![
            exit_outcome(success_status(), " M foo\n", ""),
            exit_outcome(success_status(), "", ""),
            exit_outcome(success_status(), "", ""),
            exit_outcome(success_status(), "", ""),
            exit_outcome(success_status(), "", ""),
        ]);
        let info = WorktreeInfo {
            path: PathBuf::from("/tmp/synthia-harness-fake"),
            branch: "synthia/agent-4".to_string(),
            base_sha: "abc123".to_string(),
        };
        let long_desc = "x".repeat(500);
        let _ = cleanup_worktree(&temp_base(), &info, &long_desc, &runner);
        let recorded = runner.recorded();
        let commit_msg_arg = recorded[2]
            .iter()
            .skip_while(|s| s.as_str() != "-m")
            .nth(1)
            .expect("commit message");
        assert_eq!(
            commit_msg_arg.chars().count(),
            "synthia-harness: ".len() + 200
        );
    }

    #[test]
    fn cleanup_branch_collision_appends_unique_suffix() {
        let runner = ScriptedRunner::with_outcomes(vec![
            exit_outcome(success_status(), " M foo\n", ""),
            exit_outcome(success_status(), "", ""),
            exit_outcome(success_status(), "", ""),
            // first branch attempt fails
            exit_outcome(failure_status(), "", "already exists"),
            // retry with unique suffix succeeds
            exit_outcome(success_status(), "", ""),
            exit_outcome(success_status(), "", ""),
        ]);
        let info = WorktreeInfo {
            path: PathBuf::from("/tmp/synthia-harness-fake"),
            branch: "synthia/agent-5".to_string(),
            base_sha: "abc123".to_string(),
        };
        let result = cleanup_worktree(&temp_base(), &info, "desc", &runner);
        assert!(result.has_changes);
        let branch = result.branch.expect("branch");
        assert!(branch.starts_with("synthia/agent-5-"));
    }
}
