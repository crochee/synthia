//! Tests for ask_user module

#[cfg(test)]
mod ask_user_tests {
    use async_trait::async_trait;
    use serde_json::json;

    use super::super::{
        AskUserQuestionTool,
        QuestionSenderImpl,
        types::{
            Question,
            QuestionAnswer,
            QuestionOption,
            QuestionRequest,
            QuestionResponse,
        },
    };
    use crate::{AgentError, tools::Tool};

    // =====================================================================
    // types.rs tests
    // =====================================================================

    #[test]
    fn test_question_option_serialization() {
        let opt = QuestionOption {
            label: "Yes".to_string(),
            description: "Confirm".to_string(),
        };

        let json = serde_json::to_string(&opt).unwrap();
        let parsed: QuestionOption = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.label, "Yes");
        assert_eq!(parsed.description, "Confirm");
    }

    #[test]
    fn test_question_option_with_empty_description() {
        let opt = QuestionOption {
            label: "No".to_string(),
            description: String::new(),
        };

        let json = serde_json::to_string(&opt).unwrap();
        let parsed: QuestionOption = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.label, "No");
        assert_eq!(parsed.description, "");
    }

    #[test]
    fn test_question_option_deserialization_with_missing_description() {
        let json = r#"{"label": "Maybe"}"#;
        let parsed: QuestionOption = serde_json::from_str(json).unwrap();

        assert_eq!(parsed.label, "Maybe");
        assert_eq!(parsed.description, "");
    }

    #[test]
    fn test_question_serialization() {
        let question = Question {
            question: "What is your choice?".to_string(),
            header: "Choice".to_string(),
            options: vec![
                QuestionOption {
                    label: "A".to_string(),
                    description: "Option A".to_string(),
                },
                QuestionOption {
                    label: "B".to_string(),
                    description: "Option B".to_string(),
                },
            ],
            multi_select: false,
        };

        let json = serde_json::to_string(&question).unwrap();
        let parsed: Question = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.question, "What is your choice?");
        assert_eq!(parsed.header, "Choice");
        assert_eq!(parsed.options.len(), 2);
        assert!(!parsed.multi_select);
    }

    #[test]
    fn test_question_with_default_header_and_multi_select() {
        let json = r#"{"question": "Are you sure?", "options": [{"label": "Y", "description": ""}]}"#;
        let parsed: Question = serde_json::from_str(json).unwrap();

        assert_eq!(parsed.question, "Are you sure?");
        assert_eq!(parsed.header, "");
        assert!(!parsed.multi_select);
    }

    #[test]
    fn test_question_answer_serialization() {
        let answer = QuestionAnswer {
            selected: vec!["A".to_string(), "B".to_string()],
            other: Some("custom".to_string()),
        };

        let json = serde_json::to_string(&answer).unwrap();
        let parsed: QuestionAnswer = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.selected, vec!["A", "B"]);
        assert_eq!(parsed.other, Some("custom".to_string()));
    }

    #[test]
    fn test_question_answer_without_other() {
        let answer = QuestionAnswer {
            selected: vec!["A".to_string()],
            other: None,
        };

        let json = serde_json::to_string(&answer).unwrap();
        let parsed: QuestionAnswer = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.selected, vec!["A"]);
        assert!(parsed.other.is_none());
    }

    #[test]
    fn test_question_answer_other_not_serialized_when_none() {
        let answer = QuestionAnswer {
            selected: vec!["A".to_string()],
            other: None,
        };

        let json = serde_json::to_string(&answer).unwrap();
        assert!(!json.contains("other"));
    }

    #[test]
    fn test_question_response_serialization() {
        let response = QuestionResponse {
            request_id: "req-123".to_string(),
            answers: vec![QuestionAnswer {
                selected: vec!["A".to_string()],
                other: None,
            }],
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: QuestionResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.request_id, "req-123");
        assert_eq!(parsed.answers.len(), 1);
        assert_eq!(parsed.answers[0].selected, vec!["A"]);
    }

    #[test]
    fn test_question_request_serialization() {
        let request = QuestionRequest {
            id: "id-456".to_string(),
            tool_call_id: "call-789".to_string(),
            questions: vec![Question {
                question: "Q1?".to_string(),
                header: "".to_string(),
                options: vec![QuestionOption {
                    label: "X".to_string(),
                    description: "".to_string(),
                }],
                multi_select: false,
            }],
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: QuestionRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "id-456");
        assert_eq!(parsed.tool_call_id, "call-789");
        assert_eq!(parsed.questions.len(), 1);
    }

    #[test]
    fn test_question_multi_select_serialization() {
        let question = Question {
            question: "Select all".to_string(),
            header: "Multi".to_string(),
            options: vec![
                QuestionOption {
                    label: "1".to_string(),
                    description: "".to_string(),
                },
                QuestionOption {
                    label: "2".to_string(),
                    description: "".to_string(),
                },
            ],
            multi_select: true,
        };

        let json = serde_json::to_string(&question).unwrap();
        let parsed: Question = serde_json::from_str(&json).unwrap();

        assert!(parsed.multi_select);
    }

    // =====================================================================
    // sender.rs tests
    // =====================================================================

    #[tokio::test]
    async fn test_submit_response_unknown_request_id_returns_error() {
        let sender = QuestionSenderImpl::new();

        let result = sender
            .submit_response(
                "nonexistent-id".to_string(),
                QuestionResponse {
                    request_id: "nonexistent-id".to_string(),
                    answers: vec![],
                },
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, AgentError::InvalidOperation(msg) if msg.contains("Request not found"))
        );
    }

    #[tokio::test]
    async fn test_submit_response_success() {
        use std::sync::Arc;

        use super::super::QuestionSender;

        let sender = Arc::new(QuestionSenderImpl::new());

        // Spawn a task to handle the request
        let sender_clone = Arc::clone(&sender);
        let handle = tokio::spawn(async move {
            let mut rx = sender_clone.request_rx().lock().await;
            if let Some(request) = rx.recv().await {
                // Send back a response
                let response = QuestionResponse {
                    request_id: request.id.clone(),
                    answers: vec![QuestionAnswer {
                        selected: vec!["A".to_string()],
                        other: None,
                    }],
                };
                sender_clone
                    .submit_response(request.id, response)
                    .await
                    .unwrap();
            }
        });

        // Send a question
        let request = QuestionRequest {
            id: "test-id".to_string(),
            tool_call_id: "call-id".to_string(),
            questions: vec![],
        };

        let send_result = sender.send_question(request).await;
        assert!(send_result.is_ok());

        handle.await.unwrap();
    }

    #[test]
    fn test_question_sender_impl_debug() {
        let sender = QuestionSenderImpl::new();
        let debug_str = format!("{sender:?}");
        assert!(debug_str.contains("QuestionSenderImpl"));
    }

    #[test]
    fn test_question_sender_impl_default() {
        let _sender = QuestionSenderImpl::default();
    }

    #[test]
    fn test_question_sender_impl_new() {
        let sender = QuestionSenderImpl::new();
        // Verify internal state is initialized
        let debug_str = format!("{sender:?}");
        assert!(debug_str.contains("QuestionSenderImpl"));
    }

    // =====================================================================
    // ask.rs tests - AskUserQuestionTool
    // =====================================================================

    #[derive(Debug)]
    struct MockSender {
        send_question_result: std::sync::Mutex<Option<QuestionResponse>>,
        submit_args: std::sync::Mutex<Option<QuestionRequest>>,
    }

    impl MockSender {
        fn new(response: Option<QuestionResponse>) -> Self {
            Self {
                send_question_result: std::sync::Mutex::new(response),
                submit_args: std::sync::Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl super::super::QuestionSender for MockSender {
        async fn send_question(
            &self,
            request: QuestionRequest,
        ) -> crate::Result<QuestionResponse> {
            *self.submit_args.lock().unwrap() = Some(request);
            self.send_question_result
                .lock()
                .unwrap()
                .take()
                .ok_or_else(|| {
                    crate::AgentError::InvalidOperation(
                        "Mock error".to_string(),
                    )
                })
        }
    }

    #[test]
    fn test_ask_user_question_tool_name() {
        let sender = MockSender::new(None);
        let tool = AskUserQuestionTool::new(std::sync::Arc::new(sender));

        assert_eq!(tool.name(), "askUserQuestion");
    }

    #[test]
    fn test_ask_user_question_tool_description() {
        let sender = MockSender::new(None);
        let tool = AskUserQuestionTool::new(std::sync::Arc::new(sender));

        assert!(tool.description().contains("multiple choice"));
    }

    #[test]
    fn test_ask_user_question_tool_parameters() {
        let sender = MockSender::new(None);
        let tool = AskUserQuestionTool::new(std::sync::Arc::new(sender));

        let params = tool.parameters();

        // Verify structure
        assert_eq!(params.get("type").unwrap(), "object");
        assert!(params.get("required").unwrap().is_array());

        let properties = params.get("properties").unwrap();
        assert!(properties.get("questions").is_some());
    }

    #[test]
    fn test_ask_user_question_tool_new() {
        let sender = MockSender::new(None);
        let tool = AskUserQuestionTool::new(std::sync::Arc::new(sender));

        // Just verify it constructs without panic
        assert_eq!(tool.name(), "askUserQuestion");
    }

    #[tokio::test]
    async fn test_ask_user_question_tool_call_with_invalid_args() {
        use crate::tools::Tool;

        let sender = MockSender::new(None);
        let tool = AskUserQuestionTool::new(std::sync::Arc::new(sender));

        // Pass invalid JSON args
        let invalid_args = serde_json::Value::String("not json".to_string());
        let result = tool.call(invalid_args).await;

        assert!(result.is_error == Some(true));
        let error_content = result.content;
        assert!(!error_content.is_empty());
    }

    #[tokio::test]
    async fn test_ask_user_question_tool_call_with_missing_fields() {
        use crate::tools::Tool;

        let sender = MockSender::new(None);
        let tool = AskUserQuestionTool::new(std::sync::Arc::new(sender));

        // Valid JSON but missing required "questions" field
        let args = json!({});
        let result = tool.call(args).await;

        assert!(result.is_error == Some(true));
        let error_content = result.content;
        assert!(!error_content.is_empty());
    }

    #[tokio::test]
    async fn test_ask_user_question_tool_call_with_empty_questions() {
        use crate::tools::Tool;

        let sender = MockSender::new(None);
        let tool = AskUserQuestionTool::new(std::sync::Arc::new(sender));

        // questions array is empty - minItems: 1 should fail
        let args = json!({
            "questions": []
        });
        let result = tool.call(args).await;

        // Should fail deserialization due to minItems constraint
        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_ask_user_question_tool_call_with_question_missing_options() {
        use crate::tools::Tool;

        let sender = MockSender::new(None);
        let tool = AskUserQuestionTool::new(std::sync::Arc::new(sender));

        // question without options
        let args = json!({
            "questions": [{
                "question": "What?",
                "header": "Q1"
            }]
        });
        let result = tool.call(args).await;

        // Should fail - options is required
        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_ask_user_question_tool_call_sender_error() {
        use crate::tools::Tool;

        let sender = MockSender::new(None); // Will return error
        let tool = AskUserQuestionTool::new(std::sync::Arc::new(sender));

        let args = json!({
            "questions": [{
                "question": "What?",
                "header": "Q1",
                "multiSelect": false,
                "options": [
                    {"label": "A", "description": "Option A"},
                    {"label": "B", "description": "Option B"}
                ]
            }]
        });
        let result = tool.call(args).await;

        assert!(result.is_error == Some(true));
    }

    #[tokio::test]
    async fn test_ask_user_question_tool_call_success() {
        use crate::tools::Tool;

        let response = QuestionResponse {
            request_id: "test-req".to_string(),
            answers: vec![QuestionAnswer {
                selected: vec!["A".to_string()],
                other: None,
            }],
        };
        let sender = MockSender::new(Some(response));
        let tool = AskUserQuestionTool::new(std::sync::Arc::new(sender));

        let args = json!({
            "questions": [{
                "question": "What?",
                "header": "Q1",
                "multiSelect": false,
                "options": [
                    {"label": "A", "description": "Option A"},
                    {"label": "B", "description": "Option B"}
                ]
            }]
        });
        let result = tool.call(args).await;

        assert!(result.is_error != Some(true));
    }

    #[test]
    fn test_question_sender_trait_bound() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<QuestionSenderImpl>();
    }
}
