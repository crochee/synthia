//! Report export — pretty JSON and CSV serialization for
//! [`EvalReport`].
//!
//! CSV layout: one row per case × metric, columns
//! `case_id,metric,score,passed`. A case with no scores still
//! emits one row with an empty `metric` and `score` so the row
//! count stays `≥ 1` per case. Fields containing a comma, quote,
//! or newline are quoted per RFC 4180.
//!
//! # Example
//!
//! ```rust,no_run
//! # fn example() -> Result<(), synthia_eval::EvalError> {
//! use synthia_eval::EvalReport;
//!
//! fn save(report: &EvalReport) -> Result<(), synthia_eval::EvalError> {
//!     report.export_json("/tmp/report.json")?;
//!     report.export_csv("/tmp/report.csv")?;
//!     Ok(())
//! }
//! # Ok(())
//! # }
//! ```

use std::{io::Write, path::Path};

use crate::{EvalError, EvalReport};

impl EvalReport {
    /// Render the report as pretty-printed JSON.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError::Serialization`] if the report cannot
    /// be serialized.
    pub fn to_json_string(&self) -> Result<String, EvalError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| EvalError::Serialization(e.to_string()))
    }

    /// Render the report as CSV (`case_id,metric,score,passed`).
    ///
    /// Rows are ordered by case (suite order) and then by metric
    /// name, so output is deterministic across runs.
    #[must_use]
    pub fn to_csv_string(&self) -> String {
        let mut out = String::from("case_id,metric,score,passed\n");
        for result in &self.results {
            push_csv_rows(&mut out, result);
        }
        out
    }

    /// Write the report to `path` as pretty-printed JSON.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError`] if serialization or the file write
    /// fails.
    pub fn export_json(&self, path: impl AsRef<Path>) -> Result<(), EvalError> {
        let json = self.to_json_string()?;
        write_file(path.as_ref(), &json)
    }

    /// Write the report to `path` as CSV.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError`] if the file write fails.
    pub fn export_csv(&self, path: impl AsRef<Path>) -> Result<(), EvalError> {
        write_file(path.as_ref(), &self.to_csv_string())
    }
}

/// Append one CSV row per score of `result`; cases with no scores
/// get a single row with empty metric/score.
fn push_csv_rows(out: &mut String, result: &crate::TestResult) {
    if result.scores.is_empty() {
        out.push_str(&format!(
            "{},,,{}\n",
            escape_csv(&result.case_id),
            result.passed
        ));
        return;
    }
    for (metric, score) in &result.scores {
        out.push_str(&format!(
            "{},{},{:.4},{}\n",
            escape_csv(&result.case_id),
            escape_csv(metric),
            score,
            result.passed
        ));
    }
}

/// Escape a CSV field: quote when it contains a comma, quote, or
/// newline; embedded quotes are doubled.
fn escape_csv(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn write_file(path: &Path, contents: &str) -> Result<(), EvalError> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(contents.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn pinned_timestamp() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    use crate::TestResult;

    fn report() -> EvalReport {
        let mut first = BTreeMap::new();
        first.insert("keyword".to_string(), 0.85);
        first.insert("length".to_string(), 1.0);
        let mut second = BTreeMap::new();
        second.insert("keyword".to_string(), 0.5);
        EvalReport {
            suite_name: "test_suite".into(),
            generated_at: pinned_timestamp(),
            results: vec![
                TestResult {
                    case_id: "case_1".into(),
                    actual_output: "Hello, this is a response.".into(),
                    scores: first,
                    passed: true,
                },
                TestResult {
                    case_id: "case_two, with comma".into(),
                    actual_output: "Short.".into(),
                    scores: second,
                    passed: false,
                },
            ],
            average_score: 0.78,
            passed: 1,
            total: 2,
        }
    }

    #[test]
    fn json_round_trips_every_report_field() {
        let original = report();
        let json = original.to_json_string().unwrap();
        let parsed: EvalReport = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.suite_name, "test_suite");
        assert_eq!(parsed.results.len(), 2);
        assert_eq!(parsed.passed, 1);
        assert_eq!(parsed.total, 2);
        assert!((parsed.average_score - 0.78).abs() < 1e-9);
        assert_eq!(
            parsed.results[0].actual_output,
            "Hello, this is a response."
        );
        assert!((parsed.results[0].scores["length"] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn csv_has_header_and_one_row_per_case_metric() {
        let csv = report().to_csv_string();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "case_id,metric,score,passed");
        // 2 metrics for case_1 + 1 for case_2 = 3 rows.
        assert_eq!(lines.len(), 4, "csv:\n{csv}");
        // Metric rows are sorted by name within a case.
        assert!(lines[1].starts_with("case_1,keyword,0.8500,true"), "{csv}");
        assert!(lines[2].starts_with("case_1,length,1.0000,true"), "{csv}");
        // The comma-bearing case id is quoted.
        assert!(lines[3].starts_with("\"case_two, with comma\""), "{csv}");
        assert!(lines[3].ends_with(",false"), "{csv}");
    }

    #[test]
    fn csv_escapes_quotes_in_fields() {
        assert_eq!(escape_csv("plain"), "plain");
        assert_eq!(escape_csv("with,comma"), "\"with,comma\"");
        assert_eq!(escape_csv("with\"quote"), "\"with\"\"quote\"");
        assert_eq!(escape_csv("multi\nline"), "\"multi\nline\"");
    }
    #[test]
    fn csv_case_without_scores_still_gets_a_row() {
        let report = EvalReport {
            suite_name: "s".into(),
            generated_at: pinned_timestamp(),
            results: vec![TestResult {
                case_id: "no_scores".into(),
                actual_output: String::new(),
                scores: BTreeMap::new(),
                passed: false,
            }],
            average_score: 0.0,
            passed: 0,
            total: 1,
        };
        let csv = report.to_csv_string();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1], "no_scores,,,false");
    }
    #[test]
    fn empty_report_csv_is_header_only() {
        let report = EvalReport {
            suite_name: "empty".into(),
            generated_at: pinned_timestamp(),
            average_score: 0.0,
            passed: 0,
            results: Vec::new(),
            total: 0,
        };
        let csv = report.to_csv_string();
        assert_eq!(csv, "case_id,metric,score,passed\n");
    }

    #[test]
    fn file_exports_write_the_same_bytes_as_the_string_forms() {
        let report = report();
        let json_path = std::env::temp_dir().join("synthia_eval_export.json");
        let csv_path = std::env::temp_dir().join("synthia_eval_export.csv");

        report.export_json(&json_path).unwrap();
        report.export_csv(&csv_path).unwrap();

        let json_back = std::fs::read_to_string(&json_path).unwrap();
        let csv_back = std::fs::read_to_string(&csv_path).unwrap();
        assert_eq!(json_back, report.to_json_string().unwrap());
        assert_eq!(csv_back, report.to_csv_string());

        let _ = std::fs::remove_file(&json_path);
        let _ = std::fs::remove_file(&csv_path);
    }
}
