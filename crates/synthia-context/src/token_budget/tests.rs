use super::*;

// --- Task 7.1: TokenBudget struct ---

#[test]
fn test_token_budget_default() {
    let budget = TokenBudget::new(100_000);
    assert_eq!(budget.hard_limit, 100_000);
    assert_eq!(budget.soft_limit, 100_000);
    // compaction_at = 100000 * 0.90 = 90000
    assert_eq!(budget.compaction_at, 90_000);
}

#[test]
fn test_token_budget_custom_config() {
    let config = TokenBudgetConfig::new(0.50, 0.70, 0.80);
    let budget = TokenBudget::with_config(100_000, config);
    assert_eq!(budget.compaction_at, 80_000);
}

#[test]
fn test_token_budget_config_clamping() {
    let config = TokenBudgetConfig::new(1.5, -0.1, 0.9);
    assert!((config.notice_ratio - 1.0).abs() < f64::EPSILON);
    assert!((config.warning_ratio - 0.0).abs() < f64::EPSILON);
    assert!((config.compaction_ratio - 0.9).abs() < f64::EPSILON);
}

#[test]
fn test_token_budget_config_default() {
    let config = TokenBudgetConfig::default();
    assert!((config.notice_ratio - 0.70).abs() < f64::EPSILON);
    assert!((config.warning_ratio - 0.85).abs() < f64::EPSILON);
    assert!((config.compaction_ratio - 0.90).abs() < f64::EPSILON);
}

// --- Task 7.2: TokenBudget::check ---

#[test]
fn test_check_ok() {
    let budget = TokenBudget::new(100_000);
    // 50% usage is below notice threshold (70%)
    assert_eq!(budget.check(50_000), BudgetStatus::Ok);
}

#[test]
fn test_check_notice() {
    let budget = TokenBudget::new(100_000);
    // 75% usage is above notice (70%) but below warning (85%)
    assert_eq!(budget.check(75_000), BudgetStatus::Notice);
}

#[test]
fn test_check_warning() {
    let budget = TokenBudget::new(100_000);
    // 87% usage is above warning (85%) but below compaction (90%)
    assert_eq!(budget.check(87_000), BudgetStatus::Warning);
}

#[test]
fn test_check_must_compact() {
    let budget = TokenBudget::new(100_000);
    // 92% usage is above compaction (90%)
    assert_eq!(budget.check(92_000), BudgetStatus::MustCompact);
}

#[test]
fn test_check_at_exact_thresholds() {
    let budget = TokenBudget::new(100_000);
    // Exactly at notice threshold (70000)
    assert_eq!(budget.check(70_000), BudgetStatus::Notice);
    // Exactly at warning threshold (85000)
    assert_eq!(budget.check(85_000), BudgetStatus::Warning);
    // Exactly at compaction threshold (90000)
    assert_eq!(budget.check(90_000), BudgetStatus::MustCompact);
}

#[test]
fn test_check_zero_tokens() {
    let budget = TokenBudget::new(100_000);
    assert_eq!(budget.check(0), BudgetStatus::Ok);
}

#[test]
fn test_check_over_hard_limit() {
    let budget = TokenBudget::new(100_000);
    assert_eq!(budget.check(150_000), BudgetStatus::MustCompact);
}

#[test]
fn test_remaining() {
    let budget = TokenBudget::new(100_000);
    assert_eq!(budget.remaining(60_000), 40_000);
}

#[test]
fn test_remaining_over_limit() {
    let budget = TokenBudget::new(100_000);
    assert_eq!(budget.remaining(120_000), 0);
}

#[test]
fn test_remaining_until_compaction() {
    let budget = TokenBudget::new(100_000);
    // compaction_at = 90000
    assert_eq!(budget.remaining_until_compaction(80_000), 10_000);
}

#[test]
fn test_needs_compaction() {
    let budget = TokenBudget::new(100_000);
    assert!(!budget.needs_compaction(80_000));
    assert!(budget.needs_compaction(95_000));
}

#[test]
fn test_usage_ratio() {
    let budget = TokenBudget::new(100_000);
    assert!((budget.usage_ratio(50_000) - 0.5).abs() < f64::EPSILON);
}

#[test]
fn test_usage_ratio_zero_limit() {
    let budget = TokenBudget::new(0);
    assert_eq!(budget.usage_ratio(100), 0.0);
}

// --- Task 7.3: Context safety thresholds ---

#[test]
fn test_check_context_safety_safe() {
    assert!(check_context_safety(100_000).is_ok());
    assert!(check_context_safety(32_000).is_ok());
}

#[test]
fn test_check_context_safety_warning() {
    // 20000 is below WARN_BELOW (32000) but above HARD_MIN (16000)
    assert!(check_context_safety(20_000).is_ok());
    // Warning is printed to stderr, but result is still Ok
}

#[test]
fn test_check_context_safety_rejected() {
    // 10000 is below HARD_MIN (16000)
    let result = check_context_safety(10_000);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "Context tokens below hard minimum (16000). Operation rejected. \
         Increase context window size or reduce input."
    );
}

#[test]
fn test_check_context_safety_at_hard_min() {
    // Exactly at HARD_MIN should be safe
    assert!(check_context_safety(HARD_MIN).is_ok());
}

#[test]
fn test_check_context_safety_at_warn_below() {
    // Exactly at WARN_BELOW should be safe (no warning)
    assert!(check_context_safety(WARN_BELOW).is_ok());
}

#[test]
fn test_safety_constants() {
    assert_eq!(HARD_MIN, 16_000);
    assert_eq!(WARN_BELOW, 32_000);
}
