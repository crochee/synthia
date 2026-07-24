//! Unit tests for the `types` module family.
//!
//! Coverage map (38 tests):
//!
//! - Session construction: 2 tests
//!   ([`test_session_creation`],
//!   [`test_session_with_config`]).
//! - State-machine transitions: 4 tests
//!   ([`test_valid_transition`],
//!   [`test_transition_sets_needs_save`],
//!   [`test_invalid_transition`],
//!   [`test_is_valid_transition_table`]).
//! - `TokenBudget` thresholds: 4 tests
//!   ([`test_token_budget_notice`],
//!   [`test_token_budget_warning`],
//!   [`test_token_budget_must_compact`],
//!   [`test_token_budget_ok`]).
//! - `TokenBudget` defaults: 3 tests
//!   ([`test_token_budget_default`],
//!   [`test_with_thresholds`],
//!   [`test_with_thresholds_default_matches_new`]).
//! - `TokenBudget` threshold labels: 2 tests
//!   ([`test_token_budget_threshold_name`],
//!   [`test_token_budget_threshold_value`]).
//! - `TokenBudget` boundary tests: 11 tests (one per
//!   threshold edge: just-below-soft-limit, at-soft-limit,
//!   just-below-compaction, at-compaction, just-below-hard,
//!   at-must-compact, just-below-must-compact, at-hard,
//!   above-hard, large-context, zero-tokens).
//! - `Session` budget integration: 2 tests
//!   ([`test_session_budget_status`],
//!   [`test_session_needs_compact`]).
//! - Context safety: 2 tests
//!   ([`test_context_safety_check`],
//!   [`test_context_available`]).
//! - Token-usage accumulation: 1 test
//!   ([`test_add_token_usage`]).

use super::*;

#[test]
fn test_session_creation() {
    let session = Session::new("s1".to_string());
    assert_eq!(session.id, "s1");
    assert_eq!(session.state, SessionState::Initializing);
}

#[test]
fn test_valid_transition() {
    let mut session = Session::new("s1".to_string());
    assert!(session.transition_to(SessionState::WaitingForInput).is_ok());
    assert_eq!(session.state, SessionState::WaitingForInput);
    assert!(session.needs_save);
}

#[test]
fn test_transition_sets_needs_save() {
    let mut session = Session::new("s1".to_string());
    assert!(!session.needs_save);

    session
        .transition_to(SessionState::WaitingForInput)
        .unwrap();
    assert!(session.needs_save);
}

#[test]
fn test_invalid_transition() {
    let mut session = Session::new("s1".to_string());
    assert!(session.transition_to(SessionState::Completed).is_err());
}

#[test]
fn test_token_budget_notice() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(700), TokenBudgetStatus::Notice);
}

#[test]
fn test_token_budget_warning() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(850), TokenBudgetStatus::Warning);
}

#[test]
fn test_token_budget_must_compact() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(1000), TokenBudgetStatus::MustCompact);
}

#[test]
fn test_token_budget_ok() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(500), TokenBudgetStatus::Ok);
}

#[test]
fn test_is_valid_transition_table() {
    assert!(Session::is_valid_transition(
        SessionState::Initializing,
        SessionState::WaitingForInput
    ));
    assert!(Session::is_valid_transition(
        SessionState::WaitingForInput,
        SessionState::LlmCalling
    ));
    assert!(!Session::is_valid_transition(
        SessionState::Completed,
        SessionState::LlmCalling
    ));
}

#[test]
fn test_token_budget_default() {
    let budget = TokenBudget::default();
    assert_eq!(budget.hard_limit, 128_000);
    assert_eq!(budget.soft_limit, 89_600);
    assert_eq!(budget.compaction_at, 108_800);
    assert_eq!(budget.must_compact_at, 115_200);
}

#[test]
fn test_session_budget_status() {
    let mut session = Session::new("s1".to_string());
    session.token_usage.total_tokens = 90_000;
    assert_eq!(session.budget_status(), TokenBudgetStatus::Notice);

    session.token_usage.total_tokens = 110_000;
    assert_eq!(session.budget_status(), TokenBudgetStatus::Warning);

    session.token_usage.total_tokens = 115_200;
    assert_eq!(session.budget_status(), TokenBudgetStatus::MustCompact);

    session.token_usage.total_tokens = 50_000;
    assert_eq!(session.budget_status(), TokenBudgetStatus::Ok);
}

#[test]
fn test_session_needs_compact() {
    let mut session = Session::new("s1".to_string());

    session.token_usage.total_tokens = 130_000;
    assert!(session.needs_pre_sampling_compact());
    assert!(session.needs_mid_turn_compact());

    session.token_usage.total_tokens = 50_000;
    assert!(!session.needs_pre_sampling_compact());
    assert!(!session.needs_mid_turn_compact());
}

#[test]
fn test_context_safety_check() {
    let mut session = Session::new("s1".to_string());
    session.context_window = 128_000;

    session.token_usage.total_tokens = 80_000;
    assert!(session.context_safety_check().is_ok());

    session.token_usage.total_tokens = 95_000;
    assert!(session.context_safety_check().is_ok());

    session.token_usage.total_tokens = 115_000;
    assert!(session.context_safety_check().is_err());

    session.token_usage.total_tokens = 120_000;
    assert!(session.context_safety_check().is_err());
}

#[test]
fn test_context_available() {
    let mut session = Session::new("s1".to_string());
    session.context_window = 128_000;
    session.token_usage.total_tokens = 50_000;
    assert_eq!(session.context_available(), 78_000);
}

#[test]
fn test_add_token_usage() {
    let mut session = Session::new("s1".to_string());
    session.add_token_usage(100, 200, Some(50));
    assert_eq!(session.token_usage.prompt_tokens, 100);
    assert_eq!(session.token_usage.completion_tokens, 200);
    assert_eq!(session.token_usage.total_tokens, 300);
    assert_eq!(session.token_usage.cached_prompt_tokens, Some(50));

    session.add_token_usage(50, 100, Some(25));
    assert_eq!(session.token_usage.prompt_tokens, 150);
    assert_eq!(session.token_usage.completion_tokens, 300);
    assert_eq!(session.token_usage.total_tokens, 450);
}

#[test]
fn test_session_with_config() {
    let config = SessionConfig {
        model: "claude-3-opus".to_string(),
        max_tokens: 8192,
    };
    let budget = TokenBudget::new(200_000);
    let session =
        Session::with_config("s1".to_string(), config.clone(), budget.clone());
    assert_eq!(session.config.model, "claude-3-opus");
    assert_eq!(session.token_budget.hard_limit, 200_000);
}

#[test]
fn test_token_budget_threshold_name() {
    assert_eq!(TokenBudget::threshold_name(&TokenBudgetStatus::Ok), "ok");
    assert_eq!(
        TokenBudget::threshold_name(&TokenBudgetStatus::Notice),
        "70%"
    );
    assert_eq!(
        TokenBudget::threshold_name(&TokenBudgetStatus::Warning),
        "85%"
    );
    assert_eq!(
        TokenBudget::threshold_name(&TokenBudgetStatus::MustCompact),
        "90%"
    );
}

#[test]
fn test_token_budget_threshold_value() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.threshold_value(&TokenBudgetStatus::Ok), 0);
    assert_eq!(budget.threshold_value(&TokenBudgetStatus::Notice), 700);
    assert_eq!(budget.threshold_value(&TokenBudgetStatus::Warning), 850);
    assert_eq!(budget.threshold_value(&TokenBudgetStatus::MustCompact), 900);
}

#[test]
fn test_token_budget_boundary_just_below_soft_limit() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(699), TokenBudgetStatus::Ok);
}

#[test]
fn test_token_budget_boundary_at_soft_limit() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(700), TokenBudgetStatus::Notice);
}

#[test]
fn test_token_budget_boundary_just_below_compaction() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(849), TokenBudgetStatus::Notice);
}

#[test]
fn test_token_budget_boundary_at_compaction() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(850), TokenBudgetStatus::Warning);
}

#[test]
fn test_token_budget_boundary_just_below_hard_limit() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(999), TokenBudgetStatus::MustCompact);
}

#[test]
fn test_token_budget_boundary_at_must_compact() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(900), TokenBudgetStatus::MustCompact);
}

#[test]
fn test_token_budget_boundary_just_below_must_compact() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(899), TokenBudgetStatus::Warning);
}

#[test]
fn test_token_budget_boundary_at_hard_limit() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(1000), TokenBudgetStatus::MustCompact);
}

#[test]
fn test_token_budget_boundary_above_hard_limit() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(1001), TokenBudgetStatus::MustCompact);
}

#[test]
fn test_token_budget_large_context() {
    let budget = TokenBudget::new(128_000);
    assert_eq!(budget.check(89_599), TokenBudgetStatus::Ok);
    assert_eq!(budget.check(89_600), TokenBudgetStatus::Notice);
    assert_eq!(budget.check(108_799), TokenBudgetStatus::Notice);
    assert_eq!(budget.check(108_800), TokenBudgetStatus::Warning);
    assert_eq!(budget.check(115_199), TokenBudgetStatus::Warning);
    assert_eq!(budget.check(115_200), TokenBudgetStatus::MustCompact);
    assert_eq!(budget.check(127_999), TokenBudgetStatus::MustCompact);
    assert_eq!(budget.check(128_000), TokenBudgetStatus::MustCompact);
}

#[test]
fn test_token_budget_zero_tokens() {
    let budget = TokenBudget::new(1000);
    assert_eq!(budget.check(0), TokenBudgetStatus::Ok);
}

#[test]
fn test_with_thresholds() {
    let budget = TokenBudget::with_thresholds(100_000, 0.7, 0.85);
    assert_eq!(budget.hard_limit, 100_000);
    assert_eq!(budget.soft_limit, 70_000);
    assert_eq!(budget.compaction_at, 85_000);
}

#[test]
fn test_with_thresholds_default_matches_new() {
    let new_budget = TokenBudget::new(100_000);
    let thresholds_budget = TokenBudget::with_thresholds(100_000, 0.7, 0.85);
    assert_eq!(new_budget.soft_limit, thresholds_budget.soft_limit);
    assert_eq!(new_budget.compaction_at, thresholds_budget.compaction_at);
    assert_eq!(
        new_budget.must_compact_at,
        thresholds_budget.must_compact_at
    );
}
