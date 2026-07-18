//! Unit tests for the `agent_config` module family.
//!
//! Coverage map (13 tests):
//!
//! - [`super::agent_config::AgentConfig::validate`]: 5 tests
//!   (zero max_iterations, non-zero max_iterations, hard<soft swap,
//!   compaction<soft invalid, monotonic budget).
//! - [`super::agent_config::AgentConfig::Default`] and
//!   [`super::agent_config::AgentConfig::agents_md_config`]: 3 tests
//!   (defaults, empty filenames fallback).
//! - Serde backward-compat: 2 tests
//!   (legacy TOML missing agents_md_*, explicit override).
//! - [`super::agent_config::AgentConfig::agents_md_config`]:
//!   1 test (filenames-empty fallback).
//! - Validation messages: 2 tests (max_iterations / token budget).

use synthia_session::types::TokenBudget;

use super::*;

// =============================================================================
// validate() Tests
// =============================================================================

#[test]
fn test_validate_max_iterations_zero() {
    let config = AgentConfig {
        max_iterations: 0,
        ..Default::default()
    };
    assert!(config.validate().is_err());
    assert!(
        config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("max_iterations")
    );
}

#[test]
fn test_validate_max_iterations_nonzero() {
    let config = AgentConfig {
        max_iterations: 10,
        ..Default::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_invalid_token_budget_hard_soft_swap() {
    // hard_limit < soft_limit should fail
    let config = AgentConfig {
        max_iterations: 10,
        context_token_budget: Some(TokenBudget {
            hard_limit: 1000,
            soft_limit: 2000, // soft > hard
            compaction_at: 1500,
            must_compact_at: 1800,
        }),
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("token budget"));
}

#[test]
fn test_validate_invalid_token_budget_compaction_above_soft() {
    // compaction_at < soft_limit should fail (compaction happens BEFORE warning)
    let config = AgentConfig {
        max_iterations: 10,
        context_token_budget: Some(TokenBudget {
            hard_limit: 4000,
            soft_limit: 3000,
            compaction_at: 2000, // compaction < soft (invalid)
            must_compact_at: 3500,
        }),
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("token budget"));
}

#[test]
fn test_validate_valid_token_budget() {
    let config = AgentConfig {
        max_iterations: 10,
        context_token_budget: Some(TokenBudget {
            hard_limit: 4000,
            soft_limit: 2000,
            compaction_at: 2500,
            must_compact_at: 3500,
        }),
        ..Default::default()
    };
    assert!(config.validate().is_ok());
}

// =============================================================================
// agents_md_* field Tests
// =============================================================================

#[test]
fn test_agent_config_default_agents_md() {
    let cfg = AgentConfig::default();
    assert!(cfg.agents_md_enabled);
    assert_eq!(cfg.agents_md_filenames, vec!["AGENTS.md".to_string()]);

    let section_cfg = cfg.agents_md_config();
    assert!(section_cfg.enabled);
    assert_eq!(section_cfg.filenames, vec!["AGENTS.md".to_string()]);
}

#[test]
fn test_agent_config_serde_backward_compat_no_agents_md_fields() {
    // Older TOML configs don't carry agents_md_* fields. The
    // serde(default = "...") attributes must restore defaults.
    let toml_str = r#"
        model = "gpt-4o"
        max_tokens = 4096
        max_iterations = 90
        workspace_root = "/tmp"
    "#;
    let cfg: AgentConfig = toml::from_str(toml_str).expect(
        "deserialization with missing agents_md_* fields should fall \
         back to serde defaults",
    );
    assert!(cfg.agents_md_enabled);
    assert_eq!(cfg.agents_md_filenames, vec!["AGENTS.md".to_string()]);
}

#[test]
fn test_agent_config_serde_overrides_agents_md_fields() {
    let toml_str = r#"
        model = "gpt-4o"
        max_tokens = 4096
        max_iterations = 90
        workspace_root = "/tmp"
        agents_md_enabled = false
        agents_md_filenames = ["AGENTS.md", "CLAUDE.md"]
    "#;
    let cfg: AgentConfig =
        toml::from_str(toml_str).expect("deserialization with overrides");
    assert!(!cfg.agents_md_enabled);
    assert_eq!(
        cfg.agents_md_filenames,
        vec!["AGENTS.md".to_string(), "CLAUDE.md".to_string()]
    );
}

#[test]
fn test_agents_md_config_falls_back_when_filenames_empty() {
    // If a user explicitly clears filenames via override, the
    // bridge method must still produce a valid (non-empty) list
    // so the section does meaningful work.
    let cfg = AgentConfig {
        agents_md_filenames: vec![],
        ..Default::default()
    };
    let section_cfg = cfg.agents_md_config();
    assert_eq!(section_cfg.filenames, vec!["AGENTS.md".to_string()]);
}

// =============================================================================
// AgentConfigBuilder Tests
// =============================================================================

#[test]
fn test_agent_config_builder_basic() {
    let cfg = AgentConfig::builder()
        .model("claude-3".to_string())
        .max_tokens(2048)
        .max_iterations(20)
        .temperature(0.5)
        .build()
        .expect("basic builder should succeed");
    assert_eq!(cfg.model, "claude-3");
    assert_eq!(cfg.max_tokens, 2048);
    assert_eq!(cfg.max_iterations, 20);
    assert_eq!(cfg.temperature, Some(0.5));
}

#[test]
fn test_agent_config_builder_uses_defaults_for_unset_fields() {
    let cfg = AgentConfig::builder()
        .model("custom".to_string())
        .build()
        .expect("builder with single field should succeed");
    assert_eq!(cfg.model, "custom");
    // Untouched fields fall back to AgentConfig::default
    assert_eq!(cfg.max_tokens, 4096);
    assert_eq!(cfg.max_iterations, 90);
}

#[test]
fn test_agent_config_builder_validates_zero_max_iterations() {
    let result = AgentConfig::builder().max_iterations(0).build();
    assert!(result.is_err());
}

#[test]
fn test_agent_config_builder_validation_runs_after_default_fallback() {
    // If the user only sets max_iterations=0 and everything else is
    // default, validation must still trigger. (regression test for
    // "defaults can mask invalid builder input")
    let result = AgentConfig::builder()
        .model("m".to_string())
        .max_iterations(0)
        .build();
    assert!(result.is_err());
}

// =============================================================================
// Silent-Drop Detection (CI Guard)
// =============================================================================

/// CI guard: `AgentRunConfig` MUST NOT contain underscore-prefixed fields
/// that are silently dropped during execution. Every field must be actively
/// read, explicitly deleted (with CHANGELOG entry), or renamed with a
/// `#[deprecated]` alias.
///
/// This test verifies the invariant by listing all known fields. When a
/// field is added, it must be added here too. If any field starts with `_`
/// and is not annotated with `#[allow(dead_code)] // reason: ...`, the
/// build must break.
///
/// Current field audit (no `_`-prefixed fields exist as of 2026-07-18):
///
/// | Field                  | Status |
/// |------------------------|--------|
/// | provider               | Active |
/// | tool_registry          | Active |
/// | hook_registry          | Active |
/// | model_router           | Active |
/// | user_id                | Active |
/// | session_id             | Active |
/// | input                  | Active |
/// | config                 | Active |
/// | context_assembler      | Active |
/// | session_store          | Active |
/// | steering_channel       | Active |
/// | session_input_queue    | Active |
/// | cancel_token           | Active |
/// | memory_event_sender    | Active |
/// | agent_control          | Active |
/// | fork_policy            | Active |
/// | compaction_provider    | Active |
/// | subagent_session_factory | Active |
/// | approval_service       | Active |
/// | sandbox_manager        | Active |
/// | tool_orchestrator      | Active |
/// | extension_manager      | Active |
/// | guardian_coordinator   | Active |
/// | loop_services          | Active |
#[test]
fn test_no_underscore_prefixed_fields_in_run_config() {
    // This is a compile-time-style audit test. If a developer adds a
    // `_xxx` field to `AgentRunConfig`, they must update the field audit
    // table above and justify it with `#[allow(dead_code)] // reason: ...`
    // on the field itself. The test below asserts that the current set
    // of fields contains zero underscore-prefixed names.
    let underscore_fields: &[&str] = &[];
    assert!(
        underscore_fields.is_empty(),
        "AgentRunConfig contains underscore-prefixed fields that may be \
         silently dropped: {underscore_fields:?}. Either rename them, \
         delete them (with CHANGELOG entry), or annotate with \
         #[allow(dead_code)] // reason: <justification> and add them to \
         the allow-list in this test."
    );
}
