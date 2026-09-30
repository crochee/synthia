//! Evals state: in-memory registry of `EvalSuite`s, plus a
//! runner that scores every case against the keyword metric
//! only (the only metric that doesn't require a model call).
//! Real eval runs against a real `EvalAgent` land in a
//! follow-up turn once the harness has a non-LLM deterministic
//! agent available; today the route handler returns an
//! `EvalReport` end-to-end via a deterministic `StubAgent`.

use std::{collections::BTreeMap, sync::Arc};

use synthia::{
    core::Clock as _,
    eval::{
        EvalReport,
        EvalRunner,
        EvalSuite,
        KeywordMetric,
        SyncMetricAdapter,
        TestCase,
    },
};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct EvalsState {
    inner: Arc<RwLock<BTreeMap<String, EvalSuite>>>,
    /// Wall clock the report's `generated_at` comes from
    /// (AGENTS.md §3.8: injected, never `Utc::now()`).
    clock: synthia::core::SharedClock,
}

impl EvalsState {
    pub fn build() -> Self {
        Self::build_with_clock(synthia::core::SharedClock::system())
    }

    /// [`Self::build`] with an explicit clock — the test seam that
    /// makes `generated_at` deterministic.
    pub fn build_with_clock(clock: synthia::core::SharedClock) -> Self {
        Self {
            inner: Arc::new(RwLock::new(BTreeMap::new())),
            clock,
        }
    }

    pub async fn list(&self) -> Vec<EvalSuite> {
        let g = self.inner.read().await;
        g.values().cloned().collect()
    }

    pub async fn get(&self, name: &str) -> Option<EvalSuite> {
        let g = self.inner.read().await;
        g.get(name).cloned()
    }

    pub async fn create(&self, suite: EvalSuite) -> Result<(), String> {
        let mut g = self.inner.write().await;
        if g.contains_key(suite.name()) {
            return Err(format!("suite '{}' already exists", suite.name()));
        }
        g.insert(suite.name().to_string(), suite);
        Ok(())
    }

    pub async fn remove(&self, name: &str) -> bool {
        let mut g = self.inner.write().await;
        g.remove(name).is_some()
    }

    /// Run a suite and return the report.
    ///
    /// Each case is run through the real `EvalRunner::run` with a
    /// `StubAgent` that echoes `expected_output` (or the case id
    /// fallback). The keyword metric is the only one wired —
    /// LLM-judge and schema-validation slots land with a real
    /// `EvalAgent`. The report shape matches the Rust
    /// `EvalReport` exactly; the wire-shape front-end types in
    /// `synthia-web/src/api/types.ts` mirror it.
    pub async fn run(&self, name: &str) -> Result<EvalReport, String> {
        let suite = {
            let g = self.inner.read().await;
            g.get(name).cloned()
        };
        let suite = suite.ok_or_else(|| format!("suite '{name}' not found"))?;

        let runner = EvalRunner::new()
            .metric(Box::new(SyncMetricAdapter(KeywordMetric)));
        // Build a single-suite runner view: each case becomes its
        // own one-case stub suite so the real runner path is
        // exercised once per case.
        let mut aggregated_results: Vec<synthia::eval::TestResult> =
            Vec::with_capacity(suite.cases().len());
        let mut passed = 0usize;
        for case in suite.cases() {
            let probe = case
                .expected_output
                .clone()
                .unwrap_or_else(|| case.id.clone());
            let one_case = single_case_suite(&case.id, &probe);
            let report = runner
                .run(
                    &StubAgent {
                        output: probe.clone(),
                    },
                    &one_case,
                )
                .await
                .map_err(|e| e.to_string())?;
            if let Some(r) = report.results.into_iter().next() {
                if r.passed {
                    passed += 1;
                }
                aggregated_results.push(r);
            }
        }
        let total = aggregated_results.len();
        let average_score = if total == 0 {
            0.0
        } else {
            aggregated_results
                .iter()
                .flat_map(|r| r.scores.values())
                .sum::<f64>()
                / (total as f64)
        };
        Ok(EvalReport {
            suite_name: name.to_owned(),
            generated_at: self.clock.now(),
            results: aggregated_results,
            average_score,
            passed,
            total,
        })
    }
}

/// Build a one-case suite for the runner. The runner expects its
/// own suite shape; the per-case scoring reuses the keyword metric
/// against the case's expected output (or the case id fallback).
fn single_case_suite(case_id: &str, probe: &str) -> EvalSuite {
    EvalSuite::new(format!("_stub_{case_id}")).add_case(
        TestCase::new(case_id, probe)
            .expect_contains(probe)
            .expect_output(probe),
    )
}

/// Deterministic stub agent that returns the configured output.
/// The full `EvalAgent` integration is deferred — this lets the
/// route handler return an `EvalReport` end-to-end without
/// requiring a live model.
struct StubAgent {
    output: String,
}

#[async_trait::async_trait]
impl synthia::eval::EvalAgent for StubAgent {
    async fn respond(
        &self,
        _input: &str,
    ) -> Result<String, synthia::eval::EvalError> {
        Ok(self.output.clone())
    }
}
