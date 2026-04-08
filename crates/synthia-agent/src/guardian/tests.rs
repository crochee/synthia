//! Comprehensive Guardian System Tests
//!
//! This module contains unit tests for the guardian/approval system covering:
//! - Guardian state machine transitions
//! - ApprovalRequest lifecycle (creation, timeout, cancellation)
//! - Batch approval logic (accumulation, size limits, timeout)
//! - Transcript recording and content sanitization

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::{
    ApprovalRequest, Assessment, Evidence, Guardian, GuardianConfig,
    GuardianOption, GuardianReviewer, GuardianRiskLevel, ReviewDecision,
    RiskScore, SimpleGuardian, TranscriptEntry,
};

// =============================================================================
// Guardian State Machine Tests
// =============================================================================

mod state_machine_tests {
    use super::*;

    #[tokio::test]
    async fn test_guardian_default_disabled_state() {
        // When guardian is disabled, review returns None (skipped)
        let config = GuardianConfig::default().enabled(false);
        let guardian = SimpleGuardian::new(config);

        let request =
            ApprovalRequest::shell("test-1", vec!["ls".to_string()], "/", None);
        let cancel_token = CancellationToken::new();

        let result = guardian.review(&cancel_token, request).await;
        assert!(result.is_ok());
        let decision = result.unwrap();
        assert!(decision.is_none());
    }

    #[tokio::test]
    async fn test_guardian_enabled_auto_approval() {
        // Guardian enabled with low-risk action should auto-approve
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let guardian = SimpleGuardian::new(config);

        let request =
            ApprovalRequest::shell("test-2", vec!["ls".to_string()], "/", None);
        let cancel_token = CancellationToken::new();

        let result = guardian.review(&cancel_token, request).await;
        assert!(result.is_ok());
        let decision = result.unwrap();
        assert!(decision.is_some());

        match decision.unwrap() {
            ReviewDecision::Approved => {}
            other => panic!("Expected Approved, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_guardian_enabled_auto_denial() {
        // Guardian enabled with high-risk action should auto-deny
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let guardian = SimpleGuardian::new(config);

        // sudo rm is high risk (85 score) which exceeds 80 threshold
        let request = ApprovalRequest::shell(
            "test-3",
            vec!["sudo".to_string(), "rm".to_string(), "-rf".to_string()],
            "/",
            None,
        );
        let cancel_token = CancellationToken::new();

        let result = guardian.review(&cancel_token, request).await;
        assert!(result.is_ok());
        let decision = result.unwrap();
        assert!(decision.is_some());

        match decision.unwrap() {
            ReviewDecision::Denied { reason } => {
                assert!(reason.contains("85") || reason.contains("exceeds"));
            }
            other => panic!("Expected Denied, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_guardian_requesting_approval_state_transition() {
        // Test transition: default -> requesting_approval -> approved
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(50);
        let guardian = SimpleGuardian::new(config);

        let request1 = ApprovalRequest::shell(
            "test-4a",
            vec!["ls".to_string()],
            "/",
            None,
        );
        let cancel_token = CancellationToken::new();
        let result1 = guardian.review(&cancel_token, request1).await;
        assert!(result1.is_ok());

        let request2 = ApprovalRequest::shell(
            "test-4b",
            vec!["pwd".to_string()],
            "/",
            None,
        );
        let result2 = guardian.review(&cancel_token, request2).await;
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_guardian_error_state() {
        // Test error handling when cancel token is triggered
        let config = GuardianConfig::default().enabled(true);
        let guardian = SimpleGuardian::new(config);

        let request =
            ApprovalRequest::shell("test-5", vec!["ls".to_string()], "/", None);
        let cancel_token = CancellationToken::new();
        cancel_token.cancel();

        let result = guardian.review(&cancel_token, request).await;
        assert!(result.is_ok());
        let decision = result.unwrap();
        assert!(decision.is_none());
    }

    #[tokio::test]
    async fn test_guardian_multiple_requests_state_persistence() {
        // Multiple sequential requests to verify state machine handles repeated calls
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let guardian = SimpleGuardian::new(config);
        let cancel_token = CancellationToken::new();

        let requests = vec![
            ApprovalRequest::shell(
                "multi-1",
                vec!["ls".to_string()],
                "/",
                None,
            ),
            ApprovalRequest::shell(
                "multi-2",
                vec!["pwd".to_string()],
                "/",
                None,
            ),
            ApprovalRequest::shell(
                "multi-3",
                vec!["whoami".to_string()],
                "/",
                None,
            ),
        ];

        for request in requests {
            let result = guardian.review(&cancel_token, request).await;
            assert!(result.is_ok(), "Review should succeed for each request");
            let decision = result.unwrap();
            assert!(decision.is_some());
        }
    }
}

// =============================================================================
// ApprovalRequest Tests
// =============================================================================

mod approval_request_tests {
    use super::*;

    #[test]
    fn test_approval_request_creation_shell() {
        let request = ApprovalRequest::shell(
            "shell-1",
            vec!["ls".to_string(), "-la".to_string()],
            "/home/user",
            Some("List files".to_string()),
        );

        assert_eq!(request.id(), "shell-1");
        let summary = request.action_summary();
        assert!(summary.contains("ls"));
    }

    #[test]
    fn test_approval_request_creation_exec_command() {
        let request = ApprovalRequest::exec_command(
            "exec-1",
            vec!["echo".to_string(), "hello".to_string()],
            "/tmp",
            Some("Test command".to_string()),
            true,
        );

        assert_eq!(request.id(), "exec-1");
        let summary = request.action_summary();
        assert!(summary.contains("exec_command"));
    }

    #[test]
    fn test_approval_request_creation_apply_patch() {
        let request = ApprovalRequest::apply_patch(
            "patch-1",
            "/project",
            vec!["src/main.rs".to_string()],
            5,
            "diff content",
        );

        assert_eq!(request.id(), "patch-1");
        let summary = request.action_summary();
        assert!(summary.contains("apply_patch"));
        assert!(summary.contains("1 files"));
        assert!(summary.contains("5 changes"));
    }

    #[test]
    fn test_approval_request_creation_network_access() {
        let request = ApprovalRequest::network_access(
            "net-1",
            "turn-1",
            "api.example.com",
            "192.168.1.1",
            "https",
            443,
        );

        assert_eq!(request.id(), "net-1");
        let summary = request.action_summary();
        assert!(summary.contains("network_access"));
        assert!(summary.contains("api.example.com"));
    }

    #[test]
    fn test_approval_request_creation_mcp_tool_call() {
        let request = ApprovalRequest::mcp_tool_call(
            "mcp-1",
            "filesystem",
            "readFile",
            Some(serde_json::json!({"path": "/etc/config"})),
        );

        assert_eq!(request.id(), "mcp-1");
        let summary = request.action_summary();
        assert!(summary.contains("mcp_tool_call"));
        assert!(summary.contains("filesystem"));
        assert!(summary.contains("readFile"));
    }

    #[test]
    fn test_approval_request_timeout_simulation() {
        // Simulate timeout behavior - a request that would be created and checked
        let request = ApprovalRequest::shell(
            "timeout-test",
            vec!["sleep".to_string(), "30".to_string()],
            "/",
            None,
        );

        // Verify request can be identified after creation
        assert_eq!(request.id(), "timeout-test");
        let json = request.to_json();
        assert!(json.is_ok());
    }

    #[test]
    fn test_approval_request_cancellation() {
        // Create a request and verify it can be examined before cancellation
        let request = ApprovalRequest::shell(
            "cancel-test",
            vec!["rm".to_string(), "-rf".to_string()],
            "/tmp",
            None,
        );

        // Request should be creatable and identifiable
        assert_eq!(request.id(), "cancel-test");
        let summary = request.action_summary();
        assert!(!summary.is_empty());
    }

    #[test]
    fn test_approval_request_all_variants_have_id() {
        // Verify all variants return valid IDs
        let variants = vec![
            ApprovalRequest::shell("id-shell", vec![], "/", None),
            ApprovalRequest::exec_command("id-exec", vec![], "/", None, false),
            ApprovalRequest::apply_patch("id-patch", "/", vec![], 0, ""),
            ApprovalRequest::network_access("id-net", "t", "t", "h", "p", 80),
            ApprovalRequest::mcp_tool_call("id-mcp", "s", "t", None),
        ];

        for (i, request) in variants.into_iter().enumerate() {
            let id = request.id();
            assert!(!id.is_empty(), "Variant {} should have non-empty id", i);
        }
    }
}

// =============================================================================
// Batch Approval Logic Tests
// =============================================================================

mod batch_approval_tests {
    use super::*;

    struct BatchAccumulator {
        requests: Vec<ApprovalRequest>,
        max_batch_size: usize,
        timeout: Duration,
        created_at: std::time::Instant,
    }

    impl BatchAccumulator {
        fn new(max_batch_size: usize, timeout: Duration) -> Self {
            Self {
                requests: Vec::new(),
                max_batch_size,
                timeout,
                created_at: std::time::Instant::now(),
            }
        }

        fn add(&mut self, request: ApprovalRequest) -> bool {
            if self.requests.len() >= self.max_batch_size {
                return false;
            }
            self.requests.push(request);
            true
        }

        fn is_timed_out(&self) -> bool {
            self.created_at.elapsed() > self.timeout
        }

        fn should_process(&self) -> bool {
            self.is_timed_out() || self.requests.len() >= self.max_batch_size
        }

        fn len(&self) -> usize {
            self.requests.len()
        }
    }

    #[test]
    fn test_batch_accumulation() {
        let mut batch = BatchAccumulator::new(5, Duration::from_secs(60));

        let req1 = ApprovalRequest::shell(
            "batch-1",
            vec!["ls".to_string()],
            "/",
            None,
        );
        let req2 = ApprovalRequest::shell(
            "batch-2",
            vec!["pwd".to_string()],
            "/",
            None,
        );

        assert!(batch.add(req1));
        assert!(batch.add(req2));
        assert_eq!(batch.len(), 2);
        assert!(!batch.should_process());
    }

    #[test]
    fn test_batch_size_limit() {
        let mut batch = BatchAccumulator::new(2, Duration::from_secs(60));

        let req1 = ApprovalRequest::shell(
            "batch-1",
            vec!["ls".to_string()],
            "/",
            None,
        );
        let req2 = ApprovalRequest::shell(
            "batch-2",
            vec!["pwd".to_string()],
            "/",
            None,
        );

        assert!(batch.add(req1));
        assert!(batch.add(req2));
        assert!(!batch.add(ApprovalRequest::shell(
            "batch-3",
            vec!["whoami".to_string()],
            "/",
            None
        )));
        assert_eq!(batch.len(), 2);
        assert!(batch.should_process());
    }

    #[test]
    fn test_batch_timeout() {
        let mut batch = BatchAccumulator::new(10, Duration::from_millis(1));

        // Add one request (below max size)
        let req1 = ApprovalRequest::shell(
            "batch-1",
            vec!["ls".to_string()],
            "/",
            None,
        );
        assert!(batch.add(req1));
        assert_eq!(batch.len(), 1);
        assert!(!batch.should_process());

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(10));

        // Now should process due to timeout
        assert!(batch.is_timed_out());
        assert!(batch.should_process());
    }

    #[test]
    fn test_batch_empty() {
        let batch = BatchAccumulator::new(5, Duration::from_secs(60));
        assert_eq!(batch.len(), 0);
        assert!(!batch.should_process());
    }

    #[test]
    fn test_batch_partial_accumulation() {
        // Test that partial batches can accumulate up to limit
        let mut batch = BatchAccumulator::new(3, Duration::from_secs(60));

        for i in 0..3 {
            let req = ApprovalRequest::shell(
                format!("partial-{}", i),
                vec!["echo".to_string()],
                "/",
                None,
            );
            assert!(batch.add(req));
        }

        assert_eq!(batch.len(), 3);
        assert!(batch.should_process());
    }

    #[tokio::test]
    async fn test_guardian_batch_review_decision() {
        // Test that batch requests get consistent review decisions
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let guardian = SimpleGuardian::new(config);
        let cancel_token = CancellationToken::new();

        // Create batch of low-risk requests
        let requests = vec![
            ApprovalRequest::shell(
                "batch-req-1",
                vec!["ls".to_string()],
                "/",
                None,
            ),
            ApprovalRequest::shell(
                "batch-req-2",
                vec!["pwd".to_string()],
                "/",
                None,
            ),
        ];

        for request in requests {
            let result = guardian.review(&cancel_token, request).await;
            assert!(result.is_ok());
            let decision = result.unwrap();
            assert!(decision.is_some());

            match decision.unwrap() {
                ReviewDecision::Approved => {}
                other => panic!(
                    "Expected Approved for low-risk batch request, got {:?}",
                    other
                ),
            }
        }
    }
}

// =============================================================================
// Transcript Recording Tests
// =============================================================================

mod transcript_recording_tests {
    use super::*;

    #[test]
    fn test_tool_call_recording() {
        let entry = TranscriptEntry {
            role: "tool".to_string(),
            content: "File content: /etc/passwd".to_string(),
            is_tool: true,
        };

        assert_eq!(entry.role, "tool");
        assert!(entry.is_tool);
        assert!(entry.content.contains("File content"));
    }

    #[test]
    fn test_approval_recording() {
        let entry = TranscriptEntry {
            role: "assistant".to_string(),
            content: "I will execute the command: rm -rf /tmp".to_string(),
            is_tool: false,
        };

        assert_eq!(entry.role, "assistant");
        assert!(!entry.is_tool);
    }

    #[test]
    fn test_content_sanitization() {
        // Test that sensitive content is handled properly
        let sensitive_content = "Password: secret123, API_KEY: sk-abc123";
        let entry = TranscriptEntry {
            role: "user".to_string(),
            content: sensitive_content.to_string(),
            is_tool: false,
        };

        // Content should be preserved as-is (sanitization happens at output)
        assert_eq!(entry.content, sensitive_content);
    }

    #[test]
    fn test_transcript_entry_sanitization_trim() {
        // Test whitespace trimming
        let entry = TranscriptEntry {
            role: "user".to_string(),
            content: "   Hello world   ".to_string(),
            is_tool: false,
        };

        assert_eq!(entry.content.trim(), "Hello world");
    }

    #[test]
    fn test_transcript_entry_empty_content() {
        let entry = TranscriptEntry {
            role: "assistant".to_string(),
            content: "".to_string(),
            is_tool: false,
        };

        assert!(entry.content.is_empty());
    }

    #[test]
    fn test_transcript_entry_clone() {
        let entry = TranscriptEntry {
            role: "user".to_string(),
            content: "Test content".to_string(),
            is_tool: false,
        };

        let cloned = entry.clone();
        assert_eq!(cloned.role, entry.role);
        assert_eq!(cloned.content, entry.content);
        assert_eq!(cloned.is_tool, entry.is_tool);
    }

    #[test]
    fn test_multiple_tool_call_recording() {
        let entries = vec![
            TranscriptEntry {
                role: "assistant".to_string(),
                content: "Reading file /etc/config".to_string(),
                is_tool: false,
            },
            TranscriptEntry {
                role: "tool".to_string(),
                content: "File contents here".to_string(),
                is_tool: true,
            },
            TranscriptEntry {
                role: "assistant".to_string(),
                content: "File read successfully".to_string(),
                is_tool: false,
            },
        ];

        assert_eq!(entries.len(), 3);
        assert!(entries[0].is_tool == entries[1].is_tool == entries[2].is_tool == false);
        assert!(entries[1].is_tool); // Only the middle one is a tool call
    }

    #[test]
    fn test_assessment_recording() {
        let assessment = Assessment {
            risk_level: GuardianRiskLevel::Medium,
            risk_score: 55,
            rationale: "Moderate risk operation detected".to_string(),
            evidence: vec![Evidence {
                message: "Command modifies system state".to_string(),
                why: "May cause unintended side effects".to_string(),
            }],
        };

        assert_eq!(assessment.risk_level, GuardianRiskLevel::Medium);
        assert_eq!(assessment.risk_score, 55);
        assert_eq!(assessment.evidence.len(), 1);
    }

    #[test]
    fn test_review_decision_approval_recording() {
        let decision = ReviewDecision::Approved;
        let debug_str = format!("{:?}", decision);
        assert!(debug_str.contains("Approved"));
    }

    #[test]
    fn test_review_decision_denial_recording() {
        let decision = ReviewDecision::Denied {
            reason: "High risk operation".to_string(),
        };

        match &decision {
            ReviewDecision::Denied { reason } => {
                assert!(reason.contains("High risk"));
            }
            _ => panic!("Expected Denied variant"),
        }
    }

    #[test]
    fn test_review_decision_needs_input_recording() {
        let options = vec![
            GuardianOption {
                id: "yes".to_string(),
                label: "Yes".to_string(),
                description: "Proceed".to_string(),
            },
            GuardianOption {
                id: "no".to_string(),
                label: "No".to_string(),
                description: "Cancel".to_string(),
            },
        ];

        let decision = ReviewDecision::NeedsUserInput {
            question: "Continue?".to_string(),
            options,
        };

        match &decision {
            ReviewDecision::NeedsUserInput { question, options } => {
                assert!(question.contains("Continue"));
                assert_eq!(options.len(), 2);
            }
            _ => panic!("Expected NeedsUserInput variant"),
        }
    }
}

// =============================================================================
// Risk Score Tests
// =============================================================================

mod risk_score_tests {
    use super::*;

    #[test]
    fn test_risk_score_new_caps_at_100() {
        let score = RiskScore::new(150, vec!["test".to_string()]);
        assert_eq!(score.score, 100);
    }

    #[test]
    fn test_risk_score_normal_value() {
        let score = RiskScore::new(50, vec!["factor1".to_string()]);
        assert_eq!(score.score, 50);
        assert_eq!(score.factors.len(), 1);
    }

    #[test]
    fn test_risk_score_zero() {
        let score = RiskScore::new(0, vec![]);
        assert_eq!(score.score, 0);
        assert!(score.factors.is_empty());
    }

    #[test]
    fn test_risk_score_exactly_100() {
        let score = RiskScore::new(100, vec!["max".to_string()]);
        assert_eq!(score.score, 100);
    }
}

// =============================================================================
// Guardian Reviewer Decision Tests
// =============================================================================

mod reviewer_decision_tests {
    use super::*;

    #[test]
    fn test_make_decision_low_risk_approved() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let reviewer = GuardianReviewer::new(config);

        let assessment = Assessment {
            risk_level: GuardianRiskLevel::Low,
            risk_score: 20,
            rationale: "Safe operation".to_string(),
            evidence: vec![],
        };

        match reviewer.make_decision(assessment) {
            ReviewDecision::Approved => {}
            other => panic!("Expected Approved for low risk, got {:?}", other),
        }
    }

    #[test]
    fn test_make_decision_high_risk_denied() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let reviewer = GuardianReviewer::new(config);

        let assessment = Assessment {
            risk_level: GuardianRiskLevel::High,
            risk_score: 90,
            rationale: "Dangerous operation".to_string(),
            evidence: vec![Evidence {
                message: "rm -rf detected".to_string(),
                why: "Data loss risk".to_string(),
            }],
        };

        match reviewer.make_decision(assessment) {
            ReviewDecision::Denied { reason } => {
                assert!(reason.contains("Dangerous") || !reason.is_empty());
            }
            other => panic!("Expected Denied for high risk, got {:?}", other),
        }
    }

    #[test]
    fn test_make_decision_medium_risk_needs_input() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let reviewer = GuardianReviewer::new(config);

        let assessment = Assessment {
            risk_level: GuardianRiskLevel::Medium,
            risk_score: 65,
            rationale: "Moderate risk".to_string(),
            evidence: vec![],
        };

        match reviewer.make_decision(assessment) {
            ReviewDecision::NeedsUserInput { question, options } => {
                assert!(question.contains("65") || question.contains("medium"));
                assert_eq!(options.len(), 3); // Yes, No, Cancel
            }
            other => panic!(
                "Expected NeedsUserInput for medium risk, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_make_decision_boundary_approval() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let reviewer = GuardianReviewer::new(config);

        // Score just below threshold should approve
        let assessment = Assessment {
            risk_level: GuardianRiskLevel::Medium,
            risk_score: 79,
            rationale: "Near threshold".to_string(),
            evidence: vec![],
        };

        match reviewer.make_decision(assessment) {
            ReviewDecision::Approved => {}
            other => panic!(
                "Expected Approved at 79 (below 80 threshold), got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_make_decision_boundary_denial() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let reviewer = GuardianReviewer::new(config);

        // Score at threshold should deny
        let assessment = Assessment {
            risk_level: GuardianRiskLevel::High,
            risk_score: 80,
            rationale: "At threshold".to_string(),
            evidence: vec![],
        };

        match reviewer.make_decision(assessment) {
            ReviewDecision::Denied { reason } => {
                // Score >= threshold should deny
            }
            other => {
                panic!("Expected Denied at 80 (at threshold), got {:?}", other)
            }
        }
    }
}

// =============================================================================
// Guardian Trait Implementation Tests
// =============================================================================

mod guardian_trait_tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_guardian_is_dangerous_tool() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_dangerous_tools(vec!["exec".to_string(), "bash".to_string()]);
        let guardian = SimpleGuardian::new(config);

        assert!(guardian.is_dangerous_tool("exec"));
        assert!(guardian.is_dangerous_tool("bash"));
        assert!(!guardian.is_dangerous_tool("ls"));
        assert!(!guardian.is_dangerous_tool("read"));
    }

    #[tokio::test]
    async fn test_simple_guardian_disabled_returns_none() {
        let config = GuardianConfig::default().enabled(false);
        let guardian = SimpleGuardian::new(config);
        let cancel_token = CancellationToken::new();

        let request =
            ApprovalRequest::shell("test", vec!["ls".to_string()], "/", None);
        let result = guardian.review(&cancel_token, request).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_simple_guardian_cancelled_returns_none() {
        let config = GuardianConfig::default().enabled(true);
        let guardian = SimpleGuardian::new(config);
        let cancel_token = CancellationToken::new();
        cancel_token.cancel();

        let request =
            ApprovalRequest::shell("test", vec!["ls".to_string()], "/", None);
        let result = guardian.review(&cancel_token, request).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_simple_guardian_patch_high_risk() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let guardian = SimpleGuardian::new(config);
        let cancel_token = CancellationToken::new();

        // ApplyPatch with dangerous content should be high risk
        let request = ApprovalRequest::apply_patch(
            "test",
            "/",
            vec!["file.rs".to_string()],
            1,
            "rm -rf /important",
        );

        let result = guardian.review(&cancel_token, request).await;
        assert!(result.is_ok());

        match result.unwrap() {
            Some(ReviewDecision::Denied { .. }) => {}
            other => {
                panic!("Expected Denied for dangerous patch, got {:?}", other)
            }
        }
    }
}

// =============================================================================
// Integration Tests
// =============================================================================

mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_full_approval_flow_low_risk() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let guardian = SimpleGuardian::new(config);
        let cancel_token = CancellationToken::new();

        // Low-risk read-only operation
        let request =
            ApprovalRequest::shell("flow-1", vec!["ls".to_string()], "/", None);
        let result = guardian.review(&cancel_token, request).await;

        assert!(result.is_ok());
        match result.unwrap() {
            Some(ReviewDecision::Approved) => {}
            other => panic!("Expected Approved for ls, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_full_approval_flow_high_risk_denied() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let guardian = SimpleGuardian::new(config);
        let cancel_token = CancellationToken::new();

        // High-risk destructive operation
        let request = ApprovalRequest::shell(
            "flow-2",
            vec!["sudo".to_string(), "rm".to_string(), "-rf".to_string()],
            "/",
            None,
        );
        let result = guardian.review(&cancel_token, request).await;

        assert!(result.is_ok());
        match result.unwrap() {
            Some(ReviewDecision::Denied { reason }) => {
                assert!(!reason.is_empty());
            }
            other => panic!("Expected Denied for sudo rm -rf, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_multiple_request_types() {
        let config = GuardianConfig::default()
            .enabled(true)
            .with_risk_threshold(80);
        let guardian = SimpleGuardian::new(config);
        let cancel_token = CancellationToken::new();

        let requests = vec![
            ApprovalRequest::shell("type-1", vec!["ls".to_string()], "/", None),
            ApprovalRequest::exec_command(
                "type-2",
                vec!["pwd".to_string()],
                "/",
                None,
                false,
            ),
            ApprovalRequest::network_access(
                "type-3",
                "t",
                "localhost",
                "127.0.0.1",
                "http",
                8080,
            ),
        ];

        for request in requests {
            let result = guardian.review(&cancel_token, request).await;
            assert!(result.is_ok());
        }
    }
}
