use std::process::Stdio;

use synthia_sandbox::{
    SandboxAttempt,
    SandboxManager,
    SandboxPolicy,
    backends::bubblewrap::BubblewrapBackend,
};

async fn bubblewrap_is_available() -> bool {
    tokio::process::Command::new("bwrap")
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn bubblewrap_denies_workspace_escape() {
    if !bubblewrap_is_available().await {
        return;
    }

    let workspace = tempfile::tempdir().unwrap();
    let backend = BubblewrapBackend::new(workspace.path());
    let attempt = backend
        .select(SandboxPolicy::Standard, "bash", "linux")
        .await
        .unwrap();

    let SandboxAttempt::Bubblewrap { .. } = attempt else {
        panic!("expected Bubblewrap attempt, got {:?}", attempt);
    };

    let mut cmd = tokio::process::Command::new("cat");
    cmd.arg("/etc/passwd")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    attempt.wrap(&mut cmd).unwrap();

    let output = cmd.output().await.unwrap();
    assert!(
        !output.status.success(),
        "reading /etc/passwd should fail inside the sandbox"
    );
}

#[tokio::test]
async fn bubblewrap_allows_workspace_read() {
    if !bubblewrap_is_available().await {
        return;
    }

    let workspace = tempfile::tempdir().unwrap();
    let inside = workspace.path().join("inside.txt");
    std::fs::write(&inside, "hello workspace").unwrap();

    let backend = BubblewrapBackend::new(workspace.path());
    let attempt = backend
        .select(SandboxPolicy::Standard, "bash", "linux")
        .await
        .unwrap();

    let SandboxAttempt::Bubblewrap { .. } = attempt else {
        panic!("expected Bubblewrap attempt, got {:?}", attempt);
    };

    let mut cmd = tokio::process::Command::new("cat");
    cmd.arg("/workspace/inside.txt")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    attempt.wrap(&mut cmd).unwrap();

    let output = cmd.output().await.unwrap();
    assert!(
        output.status.success(),
        "reading a workspace file should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "hello workspace"
    );
}
