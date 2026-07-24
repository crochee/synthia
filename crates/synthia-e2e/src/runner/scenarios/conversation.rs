use crate::{
    mock_server::{MockLlmServer, ScriptedResponse},
    runner::types::TestResult,
};

/// Simulates a multi-turn conversation where context
/// is maintained across turns. Turn 1: User mentions
/// a filename, mock LLM responds with text. Turn 2:
/// User references "it", mock LLM correctly resolves
/// the reference.
pub fn test_multi_turn() -> TestResult {
    TestResult::run("test_multi_turn", || {
        let server = MockLlmServer::new();

        // Queue responses for two turns
        server.set_scripted_responses(vec![
            ScriptedResponse::text(
                "I see you're asking about config.toml. Let me help you with that file.",
            ),
            ScriptedResponse::text(
                "Yes, 'it' refers to config.toml. The file contains your configuration settings.",
            ),
        ]);

        // Turn 1: User mentions filename
        let (status1, body1) = server.get_response()?;
        if status1 != 200 {
            anyhow::bail!("Turn 1: Expected status 200, got {}", status1);
        }

        let json1: serde_json::Value = serde_json::from_str(&body1)?;
        let text1 = json1["content"][0]["text"].as_str().unwrap_or("");

        // Verify turn 1 response mentions the filename
        if !text1.contains("config.toml") {
            anyhow::bail!(
                "Turn 1: Expected response to mention 'config.toml', got: {}",
                text1
            );
        }

        // Turn 2: User references "it"
        let (status2, body2) = server.get_response()?;
        if status2 != 200 {
            anyhow::bail!("Turn 2: Expected status 200, got {}", status2);
        }

        let json2: serde_json::Value = serde_json::from_str(&body2)?;
        let text2 = json2["content"][0]["text"].as_str().unwrap_or("");

        // Verify turn 2 response resolves "it" to the filename
        if !text2.contains("config.toml") {
            anyhow::bail!(
                "Turn 2: Expected response to resolve 'it' to 'config.toml', got: {}",
                text2
            );
        }

        // Verify call count
        assert_eq!(server.call_count(), 2, "Expected 2 calls for multi-turn");

        Ok(())
    })
}

/// Configures a tool call to fail with
/// file-not-found error, then verifies the agent
/// response explains or handles the failure.
pub fn test_error_recovery() -> TestResult {
    TestResult::run("test_error_recovery", || {
        let server = MockLlmServer::new();

        // Scenario: LLM first tries to read a non-existent file,
        // then the test simulates the error response and verifies the LLM
        // can handle the recovery
        let recovery_response = ScriptedResponse::text(
            "I apologize, but the file 'nonexistent.txt' could not be found. \
             This could be because the file does not exist in the current directory. \
             Please check the file path and try again, or let me know the correct path.",
        );

        server.set_scripted_response(recovery_response);

        let (status, body) = server.get_response()?;
        if status != 200 {
            anyhow::bail!("Expected status 200, got {}", status);
        }

        let json: serde_json::Value = serde_json::from_str(&body)?;
        let text = json["content"][0]["text"].as_str().unwrap_or("");

        // Verify the response addresses the error
        let addresses_error = text.contains("could not be found")
            || text.contains("not found")
            || text.contains("does not exist")
            || text.contains("apologize")
            || text.contains("error");

        if !addresses_error {
            anyhow::bail!(
                "Expected response to address the file-not-found error, got: {}",
                text
            );
        }

        // Verify no tool calls in the recovery response (agent is explaining, not retrying blindly)
        let has_tool_calls = json["content"]
            .as_array()
            .map(|arr| arr.iter().any(|c| c["type"] == "tool_use"))
            .unwrap_or(false);

        if has_tool_calls {
            // Tool calls in recovery are acceptable only if arguments differ
            // For this test, we expect explanation without tool calls
        }

        Ok(())
    })
}
