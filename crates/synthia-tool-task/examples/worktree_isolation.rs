//! # Worktree isolation
//!
//! Seam: `synthia_tool_task::worktree` — a delegated child runs
//! inside a temp detached `git worktree` created off the parent
//! session's HEAD. Cleanup removes a clean tree silently; a dirty
//! tree is committed onto `synthia/agent-<ulid>` in the base repo
//! so the parent can inspect it. Skips loudly when `git` is
//! unavailable.
//!
//! Run: cargo run -p synthia-tool-task --example worktree_isolation

use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

use synthia_tool_task::{
    gate::StdCommandRunner,
    worktree::{cleanup_worktree, create_worktree},
};

fn git(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn main() {
    let runner = StdCommandRunner::new();
    let temp = std::env::temp_dir();
    if !git(&temp, &["--version"]) {
        println!("worktree isolation: SKIP (git is not available)");
        return;
    }

    let repo =
        temp.join(format!("synthia-worktree-demo-{}", std::process::id()));
    let _ = fs::remove_dir_all(&repo);
    fs::create_dir_all(&repo).expect("create demo repo directory");

    assert!(git(&repo, &["init", "-q"]), "git init");
    assert!(
        git(&repo, &["config", "user.email", "demo@example.com"]),
        "git config user.email",
    );
    assert!(
        git(&repo, &["config", "user.name", "Demo Agent"]),
        "git config user.name",
    );
    fs::write(repo.join("README.md"), "demo repo\n").expect("seed file");
    assert!(git(&repo, &["add", "-A"]), "git add");
    assert!(git(&repo, &["commit", "-q", "-m", "seed"]), "git commit");

    let info = create_worktree(&repo, &runner).expect("create worktree");
    println!("worktree path: {}", info.path.display());
    println!("reserved branch: {}", info.branch);
    println!("base sha: {}", info.base_sha);

    // A child's uncommitted edit makes cleanup land a branch.
    fs::write(info.path.join("child-work.txt"), "done\n").expect("child edit");
    let cleaned = cleanup_worktree(&repo, &info, "demo subagent", &runner);
    println!(
        "cleanup: has_changes={} branch={:?}",
        cleaned.has_changes, cleaned.branch,
    );

    assert!(cleaned.has_changes, "the child edit must be preserved");
    assert_eq!(cleaned.branch.as_deref(), Some(info.branch.as_str()));
    assert!(!info.path.exists(), "the temp worktree must be removed");

    let _ = fs::remove_dir_all(&repo);
    println!("WORKTREE-ISOLATION: OK");
}
