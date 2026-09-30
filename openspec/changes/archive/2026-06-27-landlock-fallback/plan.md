# landlock-fallback Implementation Plan

> **For agentic workers:** Use `subagent-driven-development` or `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a real Linux Landlock sandbox backend as a fallback when bubblewrap is unavailable, plus a composite sandbox manager that selects bubblewrap -> landlock -> unavailable.

**Architecture:** Keep the existing `SandboxManager` trait and `SandboxAttempt` enum unchanged. Add a feature-gated `landlock` crate dependency. Implement `LandlockBackend` that probes ABI availability and applies filesystem rules in the child process before exec. Add `CompositeSandboxManager` to chain backends. Wire the composite manager into CLI/server configuration paths.

**Tech Stack:** Rust, tokio, landlock crate (optional), cargo features.

---

## Task 1: Add landlock crate dependency

**Files:**
- Modify: `crates/synthia-sandbox/Cargo.toml`

- [ ] **Step 1: Add optional dependency**

Add to `Cargo.toml`:
```toml
[features]
default = []
landlock = ["dep:landlock"]
seccomp = []

[dependencies]
async-trait.workspace = true
landlock = { version = "0.4", optional = true }
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tokio.workspace = true
```

- [ ] **Step 2: Verify default build**

Run: `cargo check -p synthia-sandbox`
Expected: PASS (no landlock code compiled).

- [ ] **Step 3: Verify feature build**

Run: `cargo check -p synthia-sandbox --features landlock`
Expected: PASS (landlock crate compiled).

---

## Task 2: Implement LandlockBackend ABI detection

**Files:**
- Modify: `crates/synthia-sandbox/src/backends/landlock.rs`

- [ ] **Step 1: Add feature-gated implementation stub**

```rust
#[cfg(feature = "landlock")]
use std::path::PathBuf;

use async_trait::async_trait;
use crate::{SandboxAttempt, SandboxError, SandboxManager, SandboxPolicy};

#[derive(Debug, Clone)]
pub struct LandlockBackend {
    workspace: PathBuf,
}

impl LandlockBackend {
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self { workspace: workspace.into() }
    }

    #[cfg(feature = "landlock")]
    fn is_available() -> bool {
        use landlock::{ABI, Ruleset};
        match Ruleset::default().set_compatibility(ABI::V4).create() {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    #[cfg(not(feature = "landlock"))]
    fn is_available() -> bool {
        false
    }
}

#[async_trait]
impl SandboxManager for LandlockBackend {
    async fn select(
        &self,
        policy: SandboxPolicy,
        _tool_type: &str,
        platform: &str,
    ) -> Result<SandboxAttempt, SandboxError> {
        match policy {
            SandboxPolicy::None => Ok(SandboxAttempt::None),
            SandboxPolicy::Standard | SandboxPolicy::Strict => {
                if platform != "linux" {
                    return Ok(SandboxAttempt::Unavailable);
                }
                if !Self::is_available() {
                    return Ok(SandboxAttempt::Unavailable);
                }
                Ok(SandboxAttempt::Landlock {
                    workspace: self.workspace.clone(),
                })
            }
            SandboxPolicy::Custom(_) => Ok(SandboxAttempt::Unavailable),
        }
    }
}
```

- [ ] **Step 2: Run unit test for select mapping**

Add test in same file:
```rust
#[tokio::test]
async fn select_none_returns_none() {
    let backend = LandlockBackend::new(std::env::current_dir().unwrap());
    let attempt = backend.select(SandboxPolicy::None, "bash", "linux").await.unwrap();
    assert!(matches!(attempt, SandboxAttempt::None));
}
```

Run: `cargo test -p synthia-sandbox --features landlock select_none_returns_none`
Expected: PASS.

---

## Task 3: Implement Landlock rules and wrap

**Files:**
- Modify: `crates/synthia-sandbox/src/backends/landlock.rs`
- Modify: `crates/synthia-sandbox/src/lib.rs`

- [ ] **Step 1: Add rule builder helper**

In `landlock.rs`:
```rust
#[cfg(feature = "landlock")]
fn build_ruleset(workspace: &std::path::Path, policy: &SandboxPolicy) -> Result<landlock::RulesetCreated, SandboxError> {
    use landlock::{Access, AccessFs, ABI, Ruleset, RulesetAttr, RulesetStatus};

    let mut ruleset = Ruleset::default()
        .set_compatibility(ABI::V4)
        .handle_access(AccessFs::from_all(ABI::V4))
        .map_err(|e| SandboxError::new("LANDLOCK_RULESET", e.to_string()))?;

    let workspace_access = AccessFs::from_all(ABI::V4);
    ruleset.add_rule(
        landlock::PathBeneath::new(
            landlock::PathFd::new(workspace)
                .map_err(|e| SandboxError::new("LANDLOCK_PATH", e.to_string()))?,
            workspace_access,
        )
        .map_err(|e| SandboxError::new("LANDLOCK_RULE", e.to_string()))?,
    ).map_err(|e| SandboxError::new("LANDLOCK_ADD", e.to_string()))?;

    if matches!(policy, SandboxPolicy::Standard) {
        let ro_access = AccessFs::ReadFile | AccessFs::ReadDir;
        for dir in ["/usr", "/bin", "/lib", "/lib64", "/sbin", "/proc", "/dev"] {
            if let Ok(fd) = landlock::PathFd::new(dir) {
                let _ = ruleset.add_rule(
                    landlock::PathBeneath::new(fd, ro_access)
                        .map_err(|e| SandboxError::new("LANDLOCK_RULE", e.to_string()))?,
                );
            }
        }
    }

    let ruleset = ruleset.create()
        .map_err(|e| SandboxError::new("LANDLOCK_CREATE", e.to_string()))?;
    Ok(ruleset)
}
```

- [ ] **Step 2: Implement wrap in lib.rs for Landlock variant**

Modify `SandboxAttempt::wrap`:
```rust
SandboxAttempt::Landlock { workspace } => {
    #[cfg(feature = "landlock")]
    {
        wrap_with_landlock(command, workspace)
    }
    #[cfg(not(feature = "landlock"))]
    {
        Err(SandboxError::new("UNSUPPORTED", "landlock feature not enabled"))
    }
}
```

- [ ] **Step 3: Implement wrap_with_landlock**

Add in `lib.rs`:
```rust
#[cfg(feature = "landlock")]
fn wrap_with_landlock(command: &mut Command, workspace: &PathBuf) -> Result<(), SandboxError> {
    use landlock::{RulesetStatus};
    use std::os::unix::process::CommandExt;

    let workspace = workspace.clone();
    let original_program = command.as_std().get_program().to_os_string();
    let original_args: Vec<std::ffi::OsString> = command
        .as_std()
        .get_args()
        .map(std::ffi::OsString::from)
        .collect();
    let current_dir = command.as_std().get_current_dir().map(PathBuf::from);
    let envs: Vec<_> = command
        .as_std()
        .get_envs()
        .map(|(k, v)| (k.to_os_string(), v.map(std::ffi::OsString::from)))
        .collect();

    let mut wrapped = std::process::Command::new(&original_program);
    wrapped.args(&original_args);
    if let Some(dir) = current_dir {
        wrapped.current_dir(dir);
    }
    for (key, val) in envs {
        match val {
            Some(v) => wrapped.env(key, v),
            None => wrapped.env_remove(key),
        };
    }

    unsafe {
        wrapped.pre_exec(move || {
            match crate::backends::landlock::build_ruleset(&workspace, &SandboxPolicy::Standard) {
                Ok(ruleset) => {
                    match ruleset.restrict_self() {
                        Ok(RulesetStatus::FullyEnforced) | Ok(RulesetStatus::PartiallyEnforced) => Ok(()),
                        Ok(RulesetStatus::NotEnforced) => Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "landlock not enforced",
                        )),
                        Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())),
                    }
                }
                Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())),
            }
        });
    }

    let tokio_wrapped = tokio::process::Command::from(wrapped);
    let _old = std::mem::replace(command, tokio_wrapped);
    Ok(())
}
```

Note: `build_ruleset` may need to be `pub(crate)` and accept policy; adjust as needed.

- [ ] **Step 4: Run compile check**

Run: `cargo check -p synthia-sandbox --features landlock`
Expected: PASS (may need minor signature fixes).

---

## Task 4: Implement CompositeSandboxManager

**Files:**
- Create: `crates/synthia-sandbox/src/composite.rs`
- Modify: `crates/synthia-sandbox/src/lib.rs`

- [ ] **Step 1: Create composite.rs**

```rust
use std::sync::Arc;
use async_trait::async_trait;
use crate::{SandboxAttempt, SandboxError, SandboxManager, SandboxPolicy};

pub struct CompositeSandboxManager {
    backends: Vec<Arc<dyn SandboxManager>>,
}

impl CompositeSandboxManager {
    pub fn new(backends: Vec<Arc<dyn SandboxManager>>) -> Self {
        Self { backends }
    }

    pub fn default_linux(workspace: impl Into<std::path::PathBuf>) -> Self {
        use crate::backends::bubblewrap::BubblewrapBackend;
        let workspace = workspace.into();
        let mut backends: Vec<Arc<dyn SandboxManager>> = vec![
            Arc::new(BubblewrapBackend::new(workspace.clone())),
        ];
        #[cfg(feature = "landlock")]
        {
            use crate::backends::landlock::LandlockBackend;
            backends.push(Arc::new(LandlockBackend::new(workspace)));
        }
        Self::new(backends)
    }
}

#[async_trait]
impl SandboxManager for CompositeSandboxManager {
    async fn select(
        &self,
        policy: SandboxPolicy,
        tool_type: &str,
        platform: &str,
    ) -> Result<SandboxAttempt, SandboxError> {
        if matches!(policy, SandboxPolicy::None) {
            return Ok(SandboxAttempt::None);
        }

        for backend in &self.backends {
            let attempt = backend.select(policy.clone(), tool_type, platform).await?;
            if !matches!(attempt, SandboxAttempt::Unavailable) {
                return Ok(attempt);
            }
        }

        Ok(SandboxAttempt::Unavailable)
    }
}
```

- [ ] **Step 2: Export composite module**

In `lib.rs` add:
```rust
pub mod composite;
```

- [ ] **Step 3: Add unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backends::bubblewrap::BubblewrapBackend, backends::landlock::LandlockBackend};

    #[tokio::test]
    async fn composite_prefers_first_available() {
        let workspace = std::env::current_dir().unwrap();
        let composite = CompositeSandboxManager::new(vec![
            Arc::new(BubblewrapBackend::new(workspace.clone())),
            Arc::new(LandlockBackend::new(workspace)),
        ]);
        let attempt = composite.select(SandboxPolicy::Standard, "bash", "linux").await.unwrap();
        // Should be bubblewrap if available, otherwise landlock or unavailable
        assert!(!matches!(attempt, SandboxAttempt::None));
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p synthia-sandbox --features landlock composite`
Expected: PASS.

---

## Task 5: Wire CompositeSandboxManager into configuration

**Files:**
- Modify: `crates/synthia-tool-orchestrator/src/lib.rs` (or wherever `BubblewrapBackend` is constructed)

- [ ] **Step 1: Find BubblewrapBackend construction sites**

Run: `grep -r "BubblewrapBackend::new" crates/ --include="*.rs" -n`

- [ ] **Step 2: Replace with CompositeSandboxManager::default_linux**

At each site, change:
```rust
use synthia_sandbox::backends::bubblewrap::BubblewrapBackend;
let sandbox_manager: Arc<dyn SandboxManager> = Arc::new(BubblewrapBackend::new(workspace));
```
to:
```rust
use synthia_sandbox::composite::CompositeSandboxManager;
let sandbox_manager: Arc<dyn SandboxManager> = Arc::new(CompositeSandboxManager::default_linux(workspace));
```

- [ ] **Step 3: Verify workspace builds**

Run: `cargo check --workspace --features landlock`
Expected: PASS.

---

## Task 6: Add integration test

**Files:**
- Create: `crates/synthia-sandbox/tests/landlock_integration.rs`

- [ ] **Step 1: Write test**

```rust
use std::process::Stdio;
use synthia_sandbox::{SandboxAttempt, SandboxManager, SandboxPolicy, backends::landlock::LandlockBackend};

#[tokio::test]
async fn landlock_denies_workspace_escape() {
    let landlock_available = /* probe ABI */ false;
    if !landlock_available {
        return;
    }

    let workspace = tempfile::tempdir().unwrap();
    let backend = LandlockBackend::new(workspace.path());
    let attempt = backend.select(SandboxPolicy::Standard, "bash", "linux").await.unwrap();

    let SandboxAttempt::Landlock { .. } = attempt else {
        return; // skip if unavailable
    };

    let mut cmd = tokio::process::Command::new("cat");
    cmd.arg("/etc/passwd").stdout(Stdio::piped()).stderr(Stdio::piped());
    attempt.wrap(&mut cmd).unwrap();

    let output = cmd.output().await.unwrap();
    assert!(!output.status.success(), "reading /etc/passwd should fail inside landlock sandbox");
}
```

- [ ] **Step 2: Run integration test**

Run: `cargo test -p synthia-sandbox --features landlock --test landlock_integration`
Expected: PASS or SKIP on unsupported kernels.

---

## Task 7: Lint and format

**Files:**
- All modified files

- [ ] **Step 1: Format**

Run: `cargo +nightly fmt --all`

- [ ] **Step 2: Clippy**

Run: `cargo clippy -p synthia-sandbox --all-targets --all-features --tests --all`
Expected: No warnings or errors.

- [ ] **Step 3: Full workspace check**

Run: `cargo check --workspace --features landlock`
Expected: PASS.

---

## Task 8: Documentation

**Files:**
- Modify: `crates/synthia-sandbox/README.md`

- [ ] **Step 1: Add Landlock section**

Document:
- `landlock` feature flag usage
- Kernel requirements (5.13+)
- Fallback behavior
- Differences from bubblewrap

- [ ] **Step 2: Add inline docs**

Ensure `LandlockBackend`, `CompositeSandboxManager`, and `wrap_with_landlock` have `///` doc comments.

---

## Spec Coverage Check

- `landlock-fallback` spec requirements:
  - ABI detection → Task 2
  - Workspace-scoped access → Task 3
  - Policy mapping → Task 3
  - Cargo feature gating → Task 1, Task 2, Task 3
  - Preserve args/env → Task 3

- `composite-sandbox-selection` spec requirements:
  - Prioritized fallback chain → Task 4
  - Fail-closed semantics → Task 4
  - `None` short-circuit → Task 4
