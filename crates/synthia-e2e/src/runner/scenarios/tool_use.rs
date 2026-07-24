use crate::{
    mock_server::{MockLlmServer, MockToolCall, ScriptedResponse},
    runner::types::TestResult,
};

/// Creates a test file with known content, configures
/// the mock LLM to respond with a read_file tool
/// call, and verifies the tool call is present.
pub fn test_tool_use() -> TestResult {
    TestResult::run("test_tool_use", || {
        // Create a test file with known content
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir.path().join("test_file.txt");
        std::fs::write(&test_file, "This is known test content.")?;

        // Configure mock LLM to respond with a read_file tool call
        let tool_call = MockToolCall {
            id: "tool_read_file_1".to_string(),
            name: "read_file".to_string(),
            input: serde_json::json!({
                "path": test_file.to_str().unwrap(),
            }),
        };

        let server = MockLlmServer::new();
        server.set_scripted_response(ScriptedResponse::with_tools(
            "I'll read the file for you.",
            vec![tool_call],
        ));

        // Simulate the agent flow
        let (status, body) = server.get_response()?;

        if status != 200 {
            anyhow::bail!("Expected status 200, got {}", status);
        }

        let json: serde_json::Value = serde_json::from_str(&body)?;

        // Verify tool call is present
        let tool_call_found = json["content"]
            .as_array()
            .map(|arr| {
                arr.iter().any(|c| {
                    c["type"] == "tool_use" && c["name"] == "read_file"
                })
            })
            .unwrap_or(false);

        if !tool_call_found {
            anyhow::bail!("Expected a read_file tool call in response");
        }

        // Verify the tool call arguments match the test file path
        let tool_node = json["content"].as_array().and_then(|arr| {
            arr.iter()
                .find(|c| c["type"] == "tool_use" && c["name"] == "read_file")
        });

        if let Some(tool) = tool_node {
            let path_arg = tool["input"]["path"].as_str().unwrap_or("");
            assert!(
                path_arg.contains("test_file.txt"),
                "Tool call path should reference test_file.txt, got: {}",
                path_arg
            );
        }

        // Verify the test file content is accessible
        let file_content = std::fs::read_to_string(&test_file)?;
        assert_eq!(file_content, "This is known test content.");

        Ok(())
    })
}
