#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::mock_server::{
        MockError,
        MockLlmServer,
        MockToolCall,
        ScriptedResponse,
    };

    #[test]
    fn test_text_response_serializes_correctly() {
        let resp = ScriptedResponse::text("Paris is the capital of France.");
        let json = resp.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["role"], "assistant");
        assert_eq!(parsed["content"][0]["type"], "text");
        assert_eq!(
            parsed["content"][0]["text"],
            "Paris is the capital of France."
        );
        assert!(parsed["error"].is_null());
    }

    #[test]
    fn test_tool_call_response_serializes_correctly() {
        let tool_call = MockToolCall {
            id: "tool_1".to_string(),
            name: "read_file".to_string(),
            input: serde_json::json!({"path": "/test.txt"}),
        };
        let resp = ScriptedResponse::with_tools(
            "Let me read the file.",
            vec![tool_call],
        );
        let json = resp.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["content"][0]["type"], "text");
        assert_eq!(parsed["content"][1]["type"], "tool_use");
        assert_eq!(parsed["content"][1]["name"], "read_file");
    }

    #[test]
    fn test_error_response_serializes_correctly() {
        let err = MockError::rate_limit(Some(30));
        let resp = ScriptedResponse::error(err.clone());
        let json = resp.to_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["error"]["code"], 429);
        assert_eq!(parsed["error"]["type"], "rate_limit_error");
    }

    #[test]
    fn test_server_returns_responses_in_order() {
        let server = MockLlmServer::new();
        server.set_scripted_responses(vec![
            ScriptedResponse::text("First response"),
            ScriptedResponse::text("Second response"),
            ScriptedResponse::text("Third response"),
        ]);

        let (status1, _) = server.get_response().unwrap();
        let (status2, _) = server.get_response().unwrap();
        let (status3, _) = server.get_response().unwrap();

        assert_eq!(status1, 200);
        assert_eq!(status2, 200);
        assert_eq!(status3, 200);
        assert_eq!(server.call_count(), 3);
    }

    #[test]
    fn test_rate_limit_after_n_calls() {
        let server = MockLlmServer::new();
        server.set_scripted_responses(vec![
            ScriptedResponse::text("OK"),
            ScriptedResponse::text("OK"),
        ]);
        server.set_rate_limit_after(2);

        let (s1, _) = server.get_response().unwrap();
        let (s2, _) = server.get_response().unwrap();
        assert_eq!(s1, 200);
        assert_eq!(s2, 200);

        // Third call should be rate limited (even though there are responses queued)
        let (s3, body) = server.get_response().unwrap();
        assert_eq!(s3, 429);
        assert!(body.contains("rate_limit_error"));
    }

    #[test]
    fn test_queue_exhaustion_error() {
        let server = MockLlmServer::new();
        server.set_scripted_response(ScriptedResponse::text("Only one"));

        server.get_response().unwrap();
        let result = server.get_response();
        assert!(result.is_err());
    }
}
