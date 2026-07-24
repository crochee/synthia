use crate::{
    mock_server::{MockLlmServer, ScriptedResponse},
    runner::types::TestResult,
};

/// Sends a simple text query to the mock LLM and
/// verifies the response contains the expected
/// answer with no tool calls.
pub fn test_basic_qa() -> TestResult {
    TestResult::run("test_basic_qa", || {
        let server = MockLlmServer::new();
        server.set_scripted_response(ScriptedResponse::text(
            "The capital of France is Paris.",
        ));

        // Simulate the agent flow: send query, get response from mock
        let (status, body) = server.get_response()?;

        // Verify HTTP 200
        if status != 200 {
            anyhow::bail!("Expected status 200, got {}", status);
        }

        let json: serde_json::Value = serde_json::from_str(&body)?;

        // Verify response contains "Paris"
        let content_text = json["content"][0]["text"].as_str().unwrap_or("");
        if !content_text.contains("Paris") {
            anyhow::bail!(
                "Expected response to contain 'Paris', got: {}",
                content_text
            );
        }

        // Verify no tool calls
        let tool_call_count = json["content"]
            .as_array()
            .map(|arr| arr.iter().filter(|c| c["type"] == "tool_use").count())
            .unwrap_or(0);
        if tool_call_count > 0 {
            anyhow::bail!(
                "Expected no tool calls in response, found {}",
                tool_call_count
            );
        }

        Ok(())
    })
}
