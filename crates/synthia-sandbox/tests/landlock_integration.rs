#![cfg(feature = "landlock")]

use std::process::Stdio;

use synthia_sandbox::{
    SandboxAttempt,
    SandboxManager,
    SandboxPolicy,
    backends::landlock::LandlockBackend,
};

#[tokio::test]
async fn landlock_standard_allows_workspace_read_and_denies_outside_read() {
    let workspace = tempfile::tempdir().unwrap();
    let backend = LandlockBackend::new(workspace.path());
    let attempt = backend
        .select(SandboxPolicy::Standard, "bash", "linux")
        .await
        .unwrap();

    let SandboxAttempt::Landlock { .. } = attempt else {
        // Landlock is not available on this host; skip the test.
        return;
    };

    // Write a file inside the workspace and verify it can be read.
    let inside_file = workspace.path().join("inside.txt");
    std::fs::write(&inside_file, "hello workspace").unwrap();

    let mut cmd = tokio::process::Command::new("cat");
    cmd.arg(&inside_file)
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

    // Verify a file outside the workspace cannot be read.
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
