//! Tests for `resolve_server_config` — the R53 wiring
//! that made the `--config <path>` named file drive every
//! config section.

use crate::state::{load_default_max_iterations, load_default_strategy};

/// RAII guard: clear `SYNTHIA_CONFIG_CONTENT` on construction and
/// restore the prior value on drop. The cargo test harness runs
/// tests in parallel; without a guard, an env-content test can
/// leak the variable into a sibling. Constructing an empty
/// guard at the start of every test in this file keeps them
/// isolated.
///
/// Acquires the process-wide [`ENV_CONTENT_LOCK`] for the guard's
/// lifetime so concurrent tests can't race on the env var.
pub(super) struct EnvContentGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    prev: Option<std::ffi::OsString>,
}

static ENV_CONTENT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl EnvContentGuard {
    /// Clear the env var for the guard's lifetime, restoring on
    /// drop. Use at the top of every test that depends on the
    /// env var being absent.
    pub(super) fn cleared() -> Self {
        let lock = ENV_CONTENT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os(crate::config::CONFIG_CONTENT_ENV);
        // SAFETY: scoped to the guard; restored on drop.
        unsafe {
            std::env::remove_var(crate::config::CONFIG_CONTENT_ENV);
        }
        Self { _lock: lock, prev }
    }

    /// Set the env var for the guard's lifetime, restoring on
    /// drop.
    pub(super) fn set(value: &str) -> Self {
        let lock = ENV_CONTENT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os(crate::config::CONFIG_CONTENT_ENV);
        // SAFETY: scoped to the guard; restored on drop.
        unsafe {
            std::env::set_var(crate::config::CONFIG_CONTENT_ENV, value);
        }
        Self { _lock: lock, prev }
    }
}

impl Drop for EnvContentGuard {
    fn drop(&mut self) {
        // SAFETY: mirrors the constructors above.
        unsafe {
            match self.prev.take() {
                Some(v) => {
                    std::env::set_var(crate::config::CONFIG_CONTENT_ENV, v)
                }
                None => std::env::remove_var(crate::config::CONFIG_CONTENT_ENV),
            }
        }
    }
}

/// R53: `--config <path>` is the deployment's configuration.
#[tokio::test]
async fn resolve_server_config_reads_the_named_file() {
    let _guard = EnvContentGuard::cleared();
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let named = elsewhere.path().join("deployment.yaml");
    std::fs::write(
        &named,
        "version: \"1.0\"\nagents:\n  reviewer:\n    max_steps: 5\n    strategy: best-of-n\n",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("config.toml"),
        r#"{"version": "1.0", "agents": {"reviewer": {"strategy": "react"}}}"#,
    )
    .unwrap();

    let cfg = crate::config::resolve_server_config(dir.path(), Some(&named))
        .expect("the named file must be read");
    let strategy = load_default_strategy(Some(&cfg), Some("reviewer"))
        .expect("the named file's strategy must resolve");
    assert_eq!(
        strategy.name(),
        "best-of-n",
        "the named file must win over config.toml"
    );
    assert_eq!(
        load_default_max_iterations(Some(&cfg), Some("reviewer")),
        Some(5),
        "the named file's max_steps must reach the run factory"
    );

    let cfg = crate::config::resolve_server_config(dir.path(), None)
        .expect("config.toml");
    assert_eq!(
        load_default_strategy(Some(&cfg), Some("reviewer"))
            .expect("strategy resolves")
            .name(),
        "react"
    );
}

/// R53: a named file that does not parse is logged and
/// skipped rather than ending the boot.
#[tokio::test]
async fn resolve_server_config_falls_through_a_broken_candidate() {
    let _guard = EnvContentGuard::cleared();
    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("broken.toml");
    std::fs::write(&broken, "{ this is not json or toml").unwrap();

    assert!(
        crate::config::resolve_server_config(dir.path(), Some(&broken))
            .is_none()
    );

    std::fs::write(
        dir.path().join("config.toml"),
        r#"{"version": "1.0", "default_agent": "reviewer"}"#,
    )
    .unwrap();
    let cfg = crate::config::resolve_server_config(dir.path(), Some(&broken))
        .expect("the next candidate must be tried");
    assert_eq!(cfg.default_agent.as_deref(), Some("reviewer"));

    let missing = dir.path().join("nope.yaml");
    let cfg = crate::config::resolve_server_config(dir.path(), Some(&missing))
        .expect("config.toml is still found");
    assert_eq!(cfg.default_agent.as_deref(), Some("reviewer"));
}

/// R62: `SYNTHIA_CONFIG_CONTENT` wins over every file candidate.
#[test]
fn resolve_server_config_prefers_env_content_over_file() {
    let _guard =
        EnvContentGuard::set("version: \"1.0\"\ndefault_agent: env-agent\n");

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        r#"{"version": "1.0", "default_agent": "file-agent"}"#,
    )
    .unwrap();

    let cfg = crate::config::resolve_server_config(dir.path(), None)
        .expect("env-content overlay must win");
    assert_eq!(
        cfg.default_agent.as_deref(),
        Some("env-agent"),
        "env-content overlay wins over config.toml"
    );
}

/// R62: a malformed `SYNTHIA_CONFIG_CONTENT` is logged and the
/// next candidate wins (fail-soft parity with file candidates).
#[test]
fn resolve_server_config_skips_malformed_env_content() {
    let _guard = EnvContentGuard::set("this is: not: valid: yaml: [");

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        r#"{"version": "1.0", "default_agent": "file-agent"}"#,
    )
    .unwrap();

    let cfg = crate::config::resolve_server_config(dir.path(), None)
        .expect("config.toml must be tried when env overlay fails to parse");
    assert_eq!(cfg.default_agent.as_deref(), Some("file-agent"));
}
