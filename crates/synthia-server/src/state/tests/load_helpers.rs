//! Tests for the `load_default_*` helpers — the
//! `agents.<name>` readers the run factory consumes:
//! compaction settings (R16) and tool restrictions (R58).

use crate::state::{
    load_default_compaction,
    load_default_tool_restriction,
    tests::resolve_server::EnvContentGuard,
};

#[tokio::test]
async fn load_default_compaction_reads_agent_table() {
    let _guard = EnvContentGuard::cleared();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        r#"{
            "version": "1.0",
            "agents": {
                "reviewer": {
                    "max_steps": 10,
                    "compaction": {
                        "enabled": true,
                        "reserve_tokens": 4096,
                        "keep_recent_tokens": 8000
                    }
                }
            }
        }"#,
    )
    .unwrap();

    let cfg = crate::config::resolve_server_config(dir.path(), None)
        .expect("the workspace config must parse");
    let loaded = load_default_compaction(Some(&cfg), Some("reviewer"))
        .expect("compaction table must parse");
    assert!(loaded.enabled);
    assert_eq!(loaded.reserve_tokens, 4_096);
    assert_eq!(loaded.keep_recent_tokens, 8_000);

    assert!(load_default_compaction(Some(&cfg), Some("nope")).is_none());
    assert!(load_default_compaction(Some(&cfg), None).is_none());
    assert!(load_default_compaction(None, Some("reviewer")).is_none());
}

/// R58: `load_default_tool_restriction` turns the agent
/// table's `allowed_tools` / `denied_tools` into the
/// restriction the run factory installs.
#[tokio::test]
async fn load_default_tool_restriction_reads_the_agent_table() {
    let _guard = EnvContentGuard::cleared();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        r#"{
            "version": "1.0",
            "agents": {
                "reviewer": {
                    "allowed_tools": ["read", "write"],
                    "denied_tools": ["write"]
                }
            }
        }"#,
    )
    .unwrap();
    let cfg = crate::config::resolve_server_config(dir.path(), None)
        .expect("config parses");
    let restriction =
        load_default_tool_restriction(Some(&cfg), Some("reviewer"))
            .expect("a non-empty pair must produce a restriction");
    assert!(restriction.is_relevant("read", false));
    assert!(
        !restriction.is_relevant("write", false),
        "the deny-list trims the allow-list"
    );
    assert!(!restriction.is_relevant("shell", false));

    std::fs::write(
        dir.path().join("config.toml"),
        r#"{"version": "1.0", "agents": {"reviewer": {"max_steps": 3}}}"#,
    )
    .unwrap();
    let cfg = crate::config::resolve_server_config(dir.path(), None).unwrap();
    assert!(
        load_default_tool_restriction(Some(&cfg), Some("reviewer")).is_none(),
        "no lists means no restriction"
    );
    assert!(load_default_tool_restriction(Some(&cfg), Some("nope")).is_none());
    assert!(load_default_tool_restriction(None, Some("reviewer")).is_none());
}

/// An allow-list alone admits exactly its members.
#[tokio::test]
async fn allow_list_admits_only_its_members() {
    let _guard = EnvContentGuard::cleared();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        r#"{
            "version": "1.0",
            "agents": {"reader": {"allowed_tools": ["read"]}}
        }"#,
    )
    .unwrap();
    let cfg = crate::config::resolve_server_config(dir.path(), None).unwrap();
    let restriction = load_default_tool_restriction(Some(&cfg), Some("reader"))
        .expect("an allow-list is a restriction");
    assert!(restriction.is_relevant("read", false));
    assert!(
        !restriction.is_relevant("shell", false),
        "an unlisted tool is not permitted"
    );
}
