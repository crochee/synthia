//! Tests for `load_default_strategy` (R50) — the
//! `agents.<name>.strategy` reader that resolves the
//! configured reasoning loop.

use crate::state::{
    load_default_strategy,
    tests::resolve_server::EnvContentGuard,
};

#[tokio::test]
async fn load_default_strategy_reads_agent_table() {
    let _guard = EnvContentGuard::cleared();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        r#"{
            "version": "1.0",
            "agents": {
                "reviewer": { "max_steps": 10, "strategy": "chain-of-thought" }
            }
        }"#,
    )
    .unwrap();

    let cfg = crate::config::resolve_server_config(dir.path(), None)
        .expect("the workspace config must parse");
    let strategy = load_default_strategy(Some(&cfg), Some("reviewer"))
        .expect("a named strategy must resolve");
    assert_eq!(strategy.name(), "chain-of-thought");

    std::fs::write(
        dir.path().join("config.toml"),
        r#"{"version": "1.0", "agents": {"reviewer": {"max_steps": 3}}}"#,
    )
    .unwrap();
    let cfg = crate::config::resolve_server_config(dir.path(), None).unwrap();
    assert!(load_default_strategy(Some(&cfg), Some("reviewer")).is_none());
    assert!(load_default_strategy(Some(&cfg), Some("nope")).is_none());
    assert!(load_default_strategy(Some(&cfg), None).is_none());
    assert!(load_default_strategy(None, Some("reviewer")).is_none());
}

/// R50: a strategy name this build does not ship is logged
/// and treated as unset.
#[tokio::test]
async fn load_default_strategy_falls_back_on_an_unknown_name() {
    let _guard = EnvContentGuard::cleared();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        r#"{"version": "1.0", "agents": {"reviewer": {"strategy": "mcts"}}}"#,
    )
    .unwrap();
    let cfg = crate::config::resolve_server_config(dir.path(), None).unwrap();
    assert!(load_default_strategy(Some(&cfg), Some("reviewer")).is_none());
}
