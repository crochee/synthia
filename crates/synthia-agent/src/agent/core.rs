//! Core Agent types and implementation

use std::sync::{Arc, RwLock};

use rmcp::model::{
    CreateMessageRequestParams,
    ModelHint,
    ModelPreferences,
    RawTextContent,
    SamplingContent,
    SamplingMessageContent,
};
use synthia_provider::collect_stream;
use tokio_util::sync::CancellationToken;

use super::loop_detector::LoopDetector;
use crate::{
    Result,
    config::{AgentConfig, AgentName, SessionConfig},
    context::ContextManager,
    guardian::Guardian,
    hooks::HookRegistry,
    model_router::ModelRouter,
    prompt::{
        EffectivePromptConfig,
        PromptBuilder,
        PromptContext,
        PromptState,
        TITLE_SYSTEM_PROMPT,
    },
    session::SessionManager,
    tools::{SkillTool, ToolRegistry},
};

#[derive(Clone)]
pub struct AgentDeps {
    pub tools: Arc<ToolRegistry>,
    pub context: Arc<dyn ContextManager>,
    pub session: Arc<dyn SessionManager>,
    pub router: Arc<dyn ModelRouter>,
    pub hooks: Arc<HookRegistry>,
    pub skills: Arc<SkillTool>,
    pub guardian: Arc<dyn Guardian>,
    pub control: Arc<super::control::AgentControl>,
}

impl std::fmt::Debug for AgentDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentDeps")
            .field("tools", &self.tools)
            .field("context", &"Arc<dyn ContextManager>")
            .field("session", &"Arc<dyn SessionManager>")
            .field("router", &"Arc<dyn ModelRouter>")
            .field("hooks", &self.hooks)
            .field("skills", &self.skills)
            .field("guardian", &"Arc<dyn Guardian>")
            .field("control", &self.control)
            .finish()
    }
}

#[derive(Clone)]
pub struct Agent {
    pub config: Arc<AgentConfig>,
    pub deps: AgentDeps,
    pub prompt_state: Arc<RwLock<PromptState>>,
    pub loop_detector: Arc<RwLock<LoopDetector>>,
}

impl Agent {
    pub fn new(config: Arc<AgentConfig>, deps: AgentDeps) -> Self {
        let loop_detector = LoopDetector::new(100, 3);

        Self {
            config,
            deps,
            loop_detector: Arc::new(RwLock::new(loop_detector)),
            prompt_state: Arc::new(RwLock::new(PromptState::new())),
        }
    }
}

impl std::fmt::Debug for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agent")
            .field("config", &self.config)
            .field("tools", &self.deps.tools)
            .field("router", &"Arc<dyn ModelRouter>")
            .field("context", &"Arc<dyn ContextManager>")
            .field("session", &"Arc<dyn SessionManager>")
            .field("hooks", &self.deps.hooks)
            .field("skills", &self.deps.skills)
            .field("guardian", &"Arc<dyn Guardian>")
            .field("control", &self.deps.control)
            .finish()
    }
}

impl Agent {
    /// Tools denied in Team Lead mode
    const LEAD_DENIED_TOOLS: &'static [&'static str] = &["claim_task"];
    /// Tools denied in Team Member mode
    const MEMBER_DENIED_TOOLS: &'static [&'static str] =
        &["task_create", "broadcast", "spawn_teammate"];
    /// Tools denied in Solo mode
    const SOLO_DENIED_TOOLS: &'static [&'static str] =
        &["spawn_teammate", "team_create", "team_assign", "claim_task"];

    /// Get the list of tools denied based on the agent name.
    fn denied_tools_for_name(name: &AgentName) -> Vec<String> {
        let tools = match name {
            AgentName::Solo => Self::SOLO_DENIED_TOOLS,
            AgentName::Lead => Self::LEAD_DENIED_TOOLS,
            AgentName::Custom(_) => Self::MEMBER_DENIED_TOOLS,
        };
        tools.iter().map(ToString::to_string).collect()
    }

    /// Get the list of tools denied based on the agent name.
    pub fn get_name_specific_denied_tools(&self) -> Vec<String> {
        Self::denied_tools_for_name(&self.config.name)
    }

    pub async fn get_filtered_tools(&self) -> Vec<rmcp::model::Tool> {
        // Get name-specific denied tools
        let name_denied = self.get_name_specific_denied_tools();

        // Merge with config denied_tools
        let mut all_denied = self.config.denied_tools.clone();
        for tool in name_denied {
            if !all_denied.contains(&tool) {
                all_denied.push(tool);
            }
        }

        let mut tools: Vec<rmcp::model::Tool> = self
            .deps
            .tools
            .filtered_tools(&self.config.allowed_tools, &all_denied)
            .await
            .into_iter()
            .map(|tool| {
                rmcp::model::Tool::new(
                    tool.name().to_string(),
                    tool.description().to_string(),
                    Arc::new(crate::tools::value_to_object(tool.parameters())),
                )
            })
            .collect();

        tools.sort_by(|a, b| a.name.cmp(&b.name));
        tools
    }

    pub async fn build_system_prompt(&self) -> String {
        let ctx = PromptContext {
            agent_name: &self.config.name,
            agent_description: &self.config.description,
            workspace_dir: &self.config.workspace_dir,
            skill_instructions: self.deps.skills.generate_instructions().await,
            is_subagent: self.config.is_subagent,
            session_id: None,
            mcp_servers: &[],
            additional_dirs: &[],
            output_style: None,
            language_preference: None,
            is_proactive_mode: false,
            model_name: None,
            knowledge_cutoff: None,
            team_info: None,
        };

        let mut state = match self.prompt_state.write() {
            Ok(s) => s,
            Err(_) => return String::new(),
        };

        let effective_config = if let Some(ref prompt) = self.config.prompt {
            EffectivePromptConfig::new().with_prompt(prompt.clone())
        } else {
            EffectivePromptConfig::new()
        };

        PromptBuilder::default_with_sections()
            .build_effective_prompt(&ctx, &mut state, effective_config)
            .unwrap_or_default()
    }

    pub async fn maybe_update_name(
        &self,
        session_config: &SessionConfig,
        cancel_token: CancellationToken,
    ) -> Result<()> {
        let session = self
            .deps
            .session
            .get_session(session_config)
            .await?
            .filter(|s| s.name.is_none())
            .ok_or_else(|| {
                crate::AgentError::context(
                    "Session not found or name already set",
                )
            })?;

        let conversation =
            self.deps.session.fix_conversation(session_config).await?;
        if conversation.is_empty() {
            return Ok(());
        }

        let result = self.deps.router.route(&conversation).await?;
        let model_info = result.config.model_info();
        let params = CreateMessageRequestParams {
            meta: None,
            task: None,
            messages: conversation,
            model_preferences: Some(ModelPreferences {
                hints: Some(vec![ModelHint {
                    name: Some(model_info.name.clone()),
                }]),
                cost_priority: None,
                intelligence_priority: None,
                speed_priority: None,
            }),
            system_prompt: Some(TITLE_SYSTEM_PROMPT.to_string()),
            include_context: None,
            temperature: model_info.temperature,
            max_tokens: model_info.max_tokens,
            stop_sequences: None,
            metadata: None,
            tools: None,
            tool_choice: None,
        };

        let stream = result.provider.stream(params, cancel_token).await?;
        let result = collect_stream(stream).await?;

        let name = Self::extract_name_from_content(&result.message.content);

        if !name.is_empty() {
            let mut updated = session;
            updated.name = Some(name.to_string());
            self.deps.session.update_session(&updated).await?;
        }

        Ok(())
    }

    fn extract_name_from_content(
        content: &SamplingContent<SamplingMessageContent>,
    ) -> String {
        match content {
            SamplingContent::Single(c) => {
                if let SamplingMessageContent::Text(t) = c
                    && !Self::is_reasoning_content(t)
                {
                    return t.text.trim().to_string();
                }
                String::new()
            }
            SamplingContent::Multiple(c) => c
                .iter()
                .filter_map(|c| {
                    let SamplingMessageContent::Text(t) = c else {
                        return None;
                    };
                    if Self::is_reasoning_content(t) {
                        None
                    } else {
                        Some(t.text.as_str())
                    }
                })
                .collect::<String>()
                .trim()
                .to_string(),
        }
    }

    fn is_reasoning_content(t: &RawTextContent) -> bool {
        t.meta
            .as_ref()
            .and_then(|m| m.0.get("type"))
            .and_then(|v| v.as_str())
            == Some("reasoning")
    }
}

#[cfg(test)]
mod tests {
    use rmcp::model::{
        RawTextContent,
        SamplingContent,
        SamplingMessageContent,
    };

    use super::*;
    use crate::agent::{
        LoopDetection,
        LoopDetector,
        LoopType,
        OperationPattern,
        Outcome,
    };

    #[test]
    fn test_extract_name_from_content_single() {
        let content = SamplingContent::Single(SamplingMessageContent::Text(
            RawTextContent {
                text: "Test Session Name".to_string(),
                meta: None,
            },
        ));

        let result = Agent::extract_name_from_content(&content);
        assert_eq!(result, "Test Session Name");
    }

    #[test]
    fn test_extract_name_from_content_multiple() {
        let content = SamplingContent::Multiple(vec![
            SamplingMessageContent::Text(RawTextContent {
                text: "First ".to_string(),
                meta: None,
            }),
            SamplingMessageContent::Text(RawTextContent {
                text: "Part ".to_string(),
                meta: None,
            }),
            SamplingMessageContent::Text(RawTextContent {
                text: "Name".to_string(),
                meta: None,
            }),
        ]);

        let result = Agent::extract_name_from_content(&content);
        assert_eq!(result, "First Part Name");
    }

    #[test]
    fn test_extract_name_from_content_with_reasoning() {
        let mut map = serde_json::Map::new();
        map.insert("type".to_string(), serde_json::json!("reasoning"));

        let content = SamplingContent::Multiple(vec![
            SamplingMessageContent::Text(RawTextContent {
                text: "Some reasoning text".to_string(),
                meta: Some(rmcp::model::Meta(map)),
            }),
            SamplingMessageContent::Text(RawTextContent {
                text: "Actual Name".to_string(),
                meta: None,
            }),
        ]);

        let result = Agent::extract_name_from_content(&content);
        assert_eq!(result, "Actual Name");
    }

    #[test]
    fn test_extract_name_from_content_empty() {
        let content = SamplingContent::Single(SamplingMessageContent::Text(
            RawTextContent {
                text: "   ".to_string(),
                meta: None,
            },
        ));

        let result = Agent::extract_name_from_content(&content);
        assert_eq!(result, "");
    }

    #[test]
    fn test_extract_name_from_content_trimming() {
        let content = SamplingContent::Single(SamplingMessageContent::Text(
            RawTextContent {
                text: "  Trimmed Name  ".to_string(),
                meta: None,
            },
        ));

        let result = Agent::extract_name_from_content(&content);
        assert_eq!(result, "Trimmed Name");
    }

    #[test]
    fn test_extract_name_skips_non_text() {
        use rmcp::model::ToolUseContent;

        let content = SamplingContent::Multiple(vec![
            SamplingMessageContent::ToolUse(ToolUseContent::new(
                "tool-1",
                "test_tool",
                serde_json::json!({})
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
            )),
            SamplingMessageContent::Text(RawTextContent {
                text: "Only Text".to_string(),
                meta: None,
            }),
        ]);

        let result = Agent::extract_name_from_content(&content);
        assert_eq!(result, "Only Text");
    }

    #[test]
    fn test_loop_detector_new() {
        let detector = LoopDetector::new(100, 3);
        assert_eq!(detector.history_len(), 0);
        assert_eq!(detector.consecutive_failures(), 0);
    }

    #[test]
    fn test_loop_detector_with_circuit_breaker() {
        let detector = LoopDetector::with_circuit_breaker(50, 5, 10);
        assert_eq!(detector.history_len(), 0);
        assert_eq!(detector.consecutive_failures(), 0);
    }

    #[test]
    fn test_loop_detector_record_updates_history() {
        let mut detector = LoopDetector::new(10, 3);

        let pattern = OperationPattern {
            tool_name: "Read".to_string(),
            args_hash: 123,
            timestamp: chrono::Utc::now(),
            outcome: Outcome::Success,
            result_hash: Some(456),
        };

        detector.record(pattern);
        assert_eq!(detector.history_len(), 1);
        assert_eq!(detector.consecutive_failures(), 0);
    }

    #[test]
    fn test_loop_detector_record_failure_increments_counter() {
        let mut detector = LoopDetector::new(10, 3);

        for _ in 0..3 {
            let pattern = OperationPattern {
                tool_name: "Write".to_string(),
                args_hash: 789,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Failure,
                result_hash: None,
            };
            detector.record(pattern);
        }

        assert_eq!(detector.consecutive_failures(), 3);
    }

    #[test]
    fn test_loop_detector_record_resets_on_success() {
        let mut detector = LoopDetector::new(10, 3);

        for _ in 0..3 {
            let pattern = OperationPattern {
                tool_name: "Write".to_string(),
                args_hash: 789,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Failure,
                result_hash: None,
            };
            detector.record(pattern);
        }
        assert_eq!(detector.consecutive_failures(), 3);

        let success_pattern = OperationPattern {
            tool_name: "Read".to_string(),
            args_hash: 111,
            timestamp: chrono::Utc::now(),
            outcome: Outcome::Success,
            result_hash: Some(222),
        };
        detector.record(success_pattern);
        assert_eq!(detector.consecutive_failures(), 0);
    }

    #[test]
    fn test_loop_detector_history_eviction() {
        let mut detector = LoopDetector::new(3, 3);

        for i in 0..5 {
            let pattern = OperationPattern {
                tool_name: format!("Tool{i}"),
                args_hash: i as u64,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            };
            detector.record(pattern);
        }

        assert_eq!(detector.history_len(), 3);
    }

    #[test]
    fn test_loop_detector_detect_generic_repeat() {
        let mut detector = LoopDetector::new(20, 3);

        for _ in 0..3 {
            let pattern = OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 123,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            };
            detector.record(pattern);
        }

        let detection = detector.detect_loop();
        assert!(detection.is_some());
        let detection = detection.unwrap();
        assert!(matches!(detection.loop_type, LoopType::GenericRepeat));
        assert_eq!(detection.tool_name, "Read");
        assert_eq!(detection.args_hash, 123);
        assert_eq!(detection.occurrences, 3);
    }

    #[test]
    fn test_loop_detector_detect_no_loop_below_threshold() {
        let mut detector = LoopDetector::new(20, 3);

        for _i in 0..2 {
            let pattern = OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 123,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            };
            detector.record(pattern);
        }

        assert!(detector.detect_loop().is_none());
    }

    #[test]
    fn test_loop_detector_detect_poll_no_progress() {
        let mut detector = LoopDetector::new(20, 3);
        let result_hash = 999u64;

        for _ in 0..3 {
            let pattern = OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 123,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: Some(result_hash),
            };
            detector.record(pattern);
        }

        let detection = detector.detect_loop();
        assert!(detection.is_some());
        let detection = detection.unwrap();
        assert!(matches!(detection.loop_type, LoopType::GenericRepeat));
    }

    #[test]
    fn test_loop_detector_detect_ping_pong() {
        let mut detector = LoopDetector::new(20, 3);

        for _ in 0..2 {
            let pattern_a = OperationPattern {
                tool_name: "Read".to_string(),
                args_hash: 1,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            };
            let pattern_b = OperationPattern {
                tool_name: "Write".to_string(),
                args_hash: 2,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Success,
                result_hash: None,
            };
            detector.record(pattern_a);
            detector.record(pattern_b);
        }

        let detection = detector.detect_loop();
        assert!(detection.is_some());
        let detection = detection.unwrap();
        assert!(matches!(detection.loop_type, LoopType::PingPong));
        assert!(detection.tool_name.contains("<->"));
    }

    #[test]
    fn test_loop_detector_detect_circuit_breaker() {
        let mut detector = LoopDetector::with_circuit_breaker(50, 3, 3);

        for i in 0..3 {
            let pattern = OperationPattern {
                tool_name: "Write".to_string(),
                args_hash: i as u64,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Failure,
                result_hash: None,
            };
            detector.record(pattern);
        }

        let detection = detector.detect_loop();
        assert!(detection.is_some());
        let detection = detection.unwrap();
        assert!(matches!(detection.loop_type, LoopType::CircuitBreaker));
        assert_eq!(detection.occurrences, 3);
    }

    #[test]
    fn test_loop_detector_reset_circuit_breaker() {
        let mut detector = LoopDetector::with_circuit_breaker(50, 3, 3);

        for i in 0..3 {
            let pattern = OperationPattern {
                tool_name: "Write".to_string(),
                args_hash: i as u64,
                timestamp: chrono::Utc::now(),
                outcome: Outcome::Failure,
                result_hash: None,
            };
            detector.record(pattern);
        }
        assert_eq!(detector.consecutive_failures(), 3);

        detector.reset_circuit_breaker();
        assert_eq!(detector.consecutive_failures(), 0);
        assert!(detector.detect_loop().is_none());
    }

    #[test]
    fn test_loop_detection_display() {
        let detection = LoopDetection {
            loop_type: LoopType::GenericRepeat,
            tool_name: "Read".to_string(),
            args_hash: 123,
            occurrences: 5,
            first_seen: 0,
            last_seen: 4,
        };
        let display = format!("{detection}");
        assert!(display.contains("Read"));
        assert!(display.contains("5"));

        let poll_detection = LoopDetection {
            loop_type: LoopType::PollNoProgress,
            tool_name: "Read".to_string(),
            args_hash: 123,
            occurrences: 3,
            first_seen: 0,
            last_seen: 2,
        };
        let poll_display = format!("{poll_detection}");
        assert!(poll_display.contains("no progress"));

        let pingpong_detection = LoopDetection {
            loop_type: LoopType::PingPong,
            tool_name: "Read <-> Write".to_string(),
            args_hash: 0,
            occurrences: 2,
            first_seen: 0,
            last_seen: 3,
        };
        let pingpong_display = format!("{pingpong_detection}");
        assert!(pingpong_display.contains("Ping-pong"));

        let circuit_detection = LoopDetection {
            loop_type: LoopType::CircuitBreaker,
            tool_name: "Write".to_string(),
            args_hash: 0,
            occurrences: 30,
            first_seen: 0,
            last_seen: 29,
        };
        let circuit_display = format!("{circuit_detection}");
        assert!(circuit_display.contains("Circuit breaker"));
    }

    #[test]
    fn test_outcome_variants() {
        assert!(matches!(Outcome::Success, Outcome::Success));
        assert!(matches!(Outcome::Failure, Outcome::Failure));
        assert!(matches!(Outcome::Pending, Outcome::Pending));
    }

    #[test]
    fn test_loop_type_variants() {
        assert!(matches!(LoopType::GenericRepeat, LoopType::GenericRepeat));
        assert!(matches!(LoopType::PollNoProgress, LoopType::PollNoProgress));
        assert!(matches!(LoopType::PingPong, LoopType::PingPong));
        assert!(matches!(LoopType::CircuitBreaker, LoopType::CircuitBreaker));
    }

    #[test]
    fn test_loop_detector_no_detection_on_empty() {
        let detector = LoopDetector::new(10, 3);
        assert!(detector.detect_loop().is_none());
    }

    #[test]
    fn test_operation_pattern_fields() {
        let pattern = OperationPattern {
            tool_name: "Read".to_string(),
            args_hash: 12345,
            timestamp: chrono::Utc::now(),
            outcome: Outcome::Success,
            result_hash: Some(67890),
        };

        assert_eq!(pattern.tool_name, "Read");
        assert_eq!(pattern.args_hash, 12345);
        assert!(pattern.result_hash.is_some());
        assert_eq!(pattern.result_hash.unwrap(), 67890);
    }

    #[test]
    fn test_denied_tools_for_name_solo() {
        let denied = Agent::denied_tools_for_name(&AgentName::Solo);
        assert!(denied.contains(&"spawn_teammate".to_string()));
        assert!(denied.contains(&"team_create".to_string()));
        assert!(denied.contains(&"team_assign".to_string()));
        assert!(denied.contains(&"claim_task".to_string()));
        assert_eq!(denied.len(), 4);
    }

    #[test]
    fn test_denied_tools_for_name_lead() {
        let denied = Agent::denied_tools_for_name(&AgentName::Lead);
        assert!(denied.contains(&"claim_task".to_string()));
        assert_eq!(denied.len(), 1);
    }

    #[test]
    fn test_denied_tools_for_name_custom() {
        let denied = Agent::denied_tools_for_name(&AgentName::Custom(
            "member".to_string(),
        ));
        assert!(denied.contains(&"task_create".to_string()));
        assert!(denied.contains(&"broadcast".to_string()));
        assert!(denied.contains(&"spawn_teammate".to_string()));
        assert_eq!(denied.len(), 3);
    }
}
