//! Shared helpers for the server's integration tests.
//!
//! Included with `mod support;` — the integration-test binaries are
//! separate processes, so a helper here is compiled into each of them
//! and never shared at runtime.

/// Environment variable the hermetic provider reads its key from.
pub const TEST_PROVIDER_KEY: &str = "SYNTHIA_BOOT_TEST_KEY";

/// Install a hermetic provider configuration in `temp`.
///
/// `AppState::new` refuses to boot with no provider configured:
/// `WorkspaceConfig::load_from_dir` falls back to the environment and
/// errors when neither `OPENAI_*` nor `ANTHROPIC_*` is set, so a test
/// that boots the production path would otherwise only pass on a
/// machine that happens to have credentials exported. These tests are
/// about *boot wiring*, not credentials, so the workspace declares a
/// provider whose key comes from a variable this process sets.
///
/// Nothing is ever sent anywhere: the provider is constructed, never
/// called.
pub fn install_test_provider(temp: &tempfile::TempDir) {
    static KEY: std::sync::Once = std::sync::Once::new();
    KEY.call_once(|| {
        // SAFETY: each integration-test binary is its own process, and
        // the variable is set exactly once, before any provider is
        // built. No other thread reads the environment concurrently.
        unsafe {
            std::env::set_var(TEST_PROVIDER_KEY, "test-key");
        }
    });

    let agents = temp.path().join(".agents");
    std::fs::create_dir_all(&agents).expect("create .agents");
    std::fs::write(
        agents.join("config.toml"),
        format!(
            r#"default_provider = "openai"
default_model = "gpt-4o"

[providers.openai]
type = "openai"
api_key_env = "{TEST_PROVIDER_KEY}"
default_model = "gpt-4o"
"#
        ),
    )
    .expect("write provider config");
}
