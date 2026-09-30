//! [`synthia_eval`] — suites, metrics, runner, export.
//!
//! [`EvalSuite`] describes cases, [`Metric`] / [`AsyncMetric`] score
//! them ([`KeywordMetric`], [`LlmJudgeMetric`],
//! [`SchemaValidationMetric`]), and [`EvalRunner`] decides pass/fail
//! against a threshold before exporting JSON or CSV for a CI job to
//! read.

pub use synthia_eval::*;
