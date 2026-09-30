//! Tests for `unapplied_agent_keys` — the R55 audit
//! that classifies which `agents.<name>` keys the boot
//! parses but does not apply.

use crate::state::unapplied_agent_keys;

#[test]
fn unapplied_agent_keys_are_classified() {
    let bare = crate::config::AgentConfig {
        description: None,
        model: None,
        max_steps: Some(10),
        allowed_tools: Vec::new(),
        denied_tools: Vec::new(),
        hidden: false,
        color: None,
        compaction: None,
        strategy: None,
        sandbox: None,
    };
    assert!(
        unapplied_agent_keys(&bare).is_empty(),
        "the three keys that work (max_steps, compaction, strategy) are \
         not reported; sandbox is now reported (R124 audit) until the \
         per-agent ShellTool re-bind lands"
    );

    let noisy = crate::config::AgentConfig {
        description: Some("read-only explorer".to_string()),
        model: Some("claude-opus".to_string()),
        max_steps: Some(10),
        // R58 wired these two, so they must not be reported.
        allowed_tools: vec!["read".to_string()],
        denied_tools: vec!["shell".to_string()],
        hidden: true,
        color: Some("#FF0000".to_string()),
        compaction: None,
        strategy: None,
        // R124: `sandbox` is parsed (R62) but not yet wired into a
        // per-run ShellTool re-bind, so it is reported as inert here
        // so the boot log names the gap.
        sandbox: Some("read-only".to_string()),
    };
    assert_eq!(
        unapplied_agent_keys(&noisy),
        vec!["description", "model", "hidden", "color", "sandbox"],
        "every inert key is named so the boot log can list them"
    );
}

/// The audit must not fire for a config that only uses keys
/// the boot applies.
#[test]
fn a_clean_agent_config_produces_no_keys() {
    let cfg: crate::config::ServerConfig = serde_json::from_str(
        r#"{
            "version": "1.0",
            "agents": {
                "reviewer": {
                    "max_steps": 10,
                    "strategy": "chain-of-thought",
                    "compaction": {
                        "enabled": true,
                        "reserve_tokens": 4096,
                        "keep_recent_tokens": 8000
                    }
                }
            }
        }"#,
    )
    .expect("config parses");
    for agent in cfg.agents.values() {
        assert!(unapplied_agent_keys(agent).is_empty());
    }
}
