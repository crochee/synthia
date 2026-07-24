use std::time::Instant;

use crate::{
    mock_server::{MockLlmServer, ScriptedResponse},
    runner::types::TestResult,
};

/// Runs N iterations of basic Q&A with mock LLM and
/// measures performance.
pub fn benchmark_performance() -> TestResult {
    TestResult::run("benchmark_performance", || {
        let iterations: usize = 100;

        let server = MockLlmServer::new();
        let mut responses = Vec::with_capacity(iterations);
        for i in 0..iterations {
            responses.push(ScriptedResponse::text(format!(
                "Response for iteration {}",
                i
            )));
        }
        server.set_scripted_responses(responses);

        let start = Instant::now();
        let mut success_count = 0;
        let mut failures = Vec::new();

        for i in 0..iterations {
            match server.get_response() {
                Ok((status, body)) => {
                    if status == 200 {
                        let json: serde_json::Value =
                            serde_json::from_str(&body)?;
                        let text =
                            json["content"][0]["text"].as_str().unwrap_or("");
                        if text.contains(&format!("iteration {}", i)) {
                            success_count += 1;
                        } else {
                            failures.push(format!(
                                "Iteration {}: unexpected response text",
                                i
                            ));
                        }
                    } else {
                        failures.push(format!(
                            "Iteration {}: status {}",
                            i, status
                        ));
                    }
                }
                Err(e) => failures.push(format!("Iteration {}: {}", i, e)),
            }
        }

        let elapsed = start.elapsed();
        let elapsed_ms = elapsed.as_millis() as u64;
        let iterations_per_second = if elapsed.as_secs_f64() > 0.0 {
            success_count as f64 / elapsed.as_secs_f64()
        } else {
            f64::INFINITY
        };

        // Tool calls per second is 0 for this benchmark (text-only responses)
        let tool_calls_per_second = 0.0;

        let result_msg = format!(
            "Benchmark complete: {}/{} iterations succeeded in {:.2}ms ({:.0} iter/s, {:.0} tool_calls/s)",
            success_count,
            iterations,
            elapsed_ms,
            iterations_per_second,
            tool_calls_per_second,
        );

        if success_count == iterations {
            Ok(())
        } else {
            anyhow::bail!("{}\nFailures: {}", result_msg, failures.join("; "));
        }
    })
}
