use crate::task::types::{TaskResult, TaskStatus};

/// Summary of multiple task executions.
pub struct AggregatedResult {
    pub total: usize,
    pub success_count: usize,
    pub error_count: usize,
    pub timeout_count: usize,
    pub artifacts: Vec<String>,
    pub combined_output: String,
    pub individual_results: Vec<TaskResult>,
}

impl AggregatedResult {
    pub fn all_succeeded(&self) -> bool {
        self.success_count == self.total
    }

    pub fn any_failed(&self) -> bool {
        self.error_count > 0 || self.timeout_count > 0
    }
}

/// Aggregate multiple task results into a summary.
pub fn aggregate_results(results: Vec<TaskResult>) -> AggregatedResult {
    let total = results.len();
    let success_count = results.iter().filter(|r| r.is_success()).count();
    let error_count = results
        .iter()
        .filter(|r| r.status == TaskStatus::Error)
        .count();
    let timeout_count = results
        .iter()
        .filter(|r| r.status == TaskStatus::Timeout)
        .count();

    let all_artifacts: Vec<String> =
        results.iter().flat_map(|r| r.artifacts.clone()).collect();

    let combined_output = results
        .iter()
        .map(|r| r.output.as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");

    AggregatedResult {
        total,
        success_count,
        error_count,
        timeout_count,
        artifacts: all_artifacts,
        combined_output,
        individual_results: results,
    }
}
