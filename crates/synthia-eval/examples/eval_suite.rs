//! Score a suite with an `EvalRunner`: built-in `KeywordMetric` plus
//! a custom `Metric` impl, exported as both JSON and CSV.
//!
//! What to look at:
//!
//! - the custom metric — a plain sync `Metric` (implement `name` +
//!   `score`), plugged in through `SyncMetricAdapter` so the runner
//!   sees it as an `AsyncMetric`. It scores output brevity, and the
//!   `Arc<parking_lot::Mutex<usize>>` counter shows the runner
//!   really invoked it once per case.
//! - the pass rule — a case passes iff **every** metric scores ≥ the
//!   threshold, so `arithmetic` and `brevity` fail on different
//!   metrics while `greeting` passes on both.
//! - the two export renderings — `to_json_string` (pretty JSON) and
//!   `to_csv_string` (one row per case × metric).
//!
//! Run: cargo run -p synthia-eval --example eval_suite

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use synthia_eval::{
    EvalAgent,
    EvalError,
    EvalRunner,
    EvalSuite,
    KeywordMetric,
    Metric,
    SyncMetricAdapter,
    TestCase,
};

/// Scores `1.0` when the output fits the word budget, decaying
/// linearly past it.
struct WordBudgetMetric {
    limit: usize,
    calls: Arc<Mutex<usize>>,
}

impl Metric for WordBudgetMetric {
    fn name(&self) -> &'static str {
        "word_budget"
    }

    fn score(
        &self,
        _input: &str,
        actual_output: &str,
        _expected_keywords: &[&str],
    ) -> f64 {
        *self.calls.lock() += 1;
        let words = actual_output.split_whitespace().count();
        if words <= self.limit {
            1.0
        } else {
            self.limit as f64 / words as f64
        }
    }
}

/// Deterministic keyless agent under test.
struct DemoAgent;

#[async_trait]
impl EvalAgent for DemoAgent {
    async fn respond(&self, input: &str) -> Result<String, EvalError> {
        Ok(match input {
            "greet" => "Hello, operator!".to_string(),
            "add two and two" => "4".to_string(),
            other => format!("Caching stores results so repeated work {other}"),
        })
    }
}

fn suite() -> EvalSuite {
    EvalSuite::new("demo-suite")
        .add_case(TestCase::new("greeting", "greet").expect_contains("hello"))
        .add_case(
            TestCase::new("arithmetic", "add two and two")
                .expect_contains("4")
                .expect_contains("four"),
        )
        .add_case(
            TestCase::new("brevity", "explain caching")
                .expect_contains("caching"),
        )
}

#[tokio::main]
async fn main() -> Result<(), EvalError> {
    let calls = Arc::new(Mutex::new(0usize));
    let custom = WordBudgetMetric {
        limit: 4,
        calls: Arc::clone(&calls),
    };

    let report = EvalRunner::new()
        .metric(Box::new(SyncMetricAdapter(KeywordMetric)))
        .metric(Box::new(SyncMetricAdapter(custom)))
        .threshold(0.7)
        .run(&DemoAgent, &suite())
        .await?;

    println!("{}", report.summary());
    println!("custom metric invocations: {}", calls.lock());

    for result in &report.results {
        println!(
            "  case {:<10} passed={:<5} scores={:?}",
            result.case_id, result.passed, result.scores
        );
    }

    println!("\n--- JSON export (first 9 lines) ---");
    for line in report.to_json_string()?.lines().take(9) {
        println!("{line}");
    }

    println!("\n--- CSV export ---");
    for line in report.to_csv_string().lines() {
        println!("{line}");
    }

    assert_eq!(report.total, 3);
    assert_eq!(report.passed, 1);
    println!("EVAL-SUITE: OK");
    Ok(())
}
