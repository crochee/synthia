//! PBAC Evaluation module - runtime evaluation and auditing.

mod audit;
mod evaluator;
mod risk;
mod types;

#[cfg(test)]
mod tests;

pub use audit::ConsoleAuditLogger;
pub use evaluator::{PolicyEvaluator, PolicyEvaluatorBuilder};
pub use risk::StandardRiskEvaluator;
pub use types::{
    AuditInfo,
    EvaluationDecision,
    EvaluationResult,
    FailedCondition,
};
