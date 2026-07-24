use std::time::Instant;

#[derive(Debug, Clone)]
pub enum TelemetryEvent {
    ToolCall {
        tool_name: String,
        tool_call_id: String,
        duration_ms: u64,
        success: bool,
        session_id: String,
    },
    ContextEvent {
        event_type: String,
        token_count: usize,
        details: String,
        session_id: String,
    },
    SkillRetrieval {
        query: String,
        matched_skills: Vec<String>,
        scores: Vec<f64>,
        duration_ms: u64,
        session_id: String,
    },
    ProviderCall {
        provider_name: String,
        model: String,
        duration_ms: u64,
        token_usage: Option<(usize, usize)>,
        cached: bool,
        session_id: String,
    },
    Error {
        component: String,
        error_message: String,
        session_id: String,
    },
}

pub struct TelemetryEmitter {
    event_count: std::sync::atomic::AtomicU64,
}

impl TelemetryEmitter {
    pub fn new() -> Self {
        Self {
            event_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn emit(&self, event: TelemetryEvent) {
        self.event_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        match &event {
            TelemetryEvent::ToolCall {
                tool_name,
                tool_call_id,
                duration_ms,
                success,
                session_id,
            } => {
                tracing::info!(
                    target: "synthia.telemetry",
                    event = "tool_call",
                    tool_name = tool_name,
                    tool_call_id = tool_call_id,
                    duration_ms = duration_ms,
                    success = success,
                    session_id = session_id,
                    "Tool call completed"
                );
            }
            TelemetryEvent::ContextEvent {
                event_type,
                token_count,
                details,
                session_id,
            } => {
                tracing::info!(
                    target: "synthia.telemetry",
                    event = "context_event",
                    event_type = event_type,
                    token_count = token_count,
                    details = details,
                    session_id = session_id,
                    "Context event"
                );
            }
            TelemetryEvent::SkillRetrieval {
                query,
                matched_skills,
                scores,
                duration_ms,
                session_id,
            } => {
                tracing::info!(
                    target: "synthia.telemetry",
                    event = "skill_retrieval",
                    query = query,
                    matched_count = matched_skills.len(),
                    duration_ms = duration_ms,
                    session_id = session_id,
                    "Skill retrieval completed"
                );
                for (skill, score) in matched_skills.iter().zip(scores.iter()) {
                    tracing::info!(
                        target: "synthia.telemetry",
                        event = "skill_match",
                        skill = skill,
                        score = score,
                        session_id = session_id,
                        "Skill match"
                    );
                }
            }
            TelemetryEvent::ProviderCall {
                provider_name,
                model,
                duration_ms,
                token_usage,
                cached,
                session_id,
            } => {
                tracing::info!(
                    target: "synthia.telemetry",
                    event = "provider_call",
                    provider = provider_name,
                    model = model,
                    duration_ms = duration_ms,
                    cached = cached,
                    session_id = session_id,
                    "Provider call completed"
                );
                if let Some((prompt, completion)) = token_usage {
                    tracing::info!(
                        target: "synthia.telemetry",
                        prompt_tokens = prompt,
                        completion_tokens = completion,
                        session_id = session_id,
                        "Token usage"
                    );
                }
            }
            TelemetryEvent::Error {
                component,
                error_message,
                session_id,
            } => {
                tracing::error!(
                    target: "synthia.telemetry",
                    event = "error",
                    component = component,
                    error = error_message,
                    session_id = session_id,
                    "Error occurred"
                );
            }
        }
    }

    pub fn event_count(&self) -> u64 {
        self.event_count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for TelemetryEmitter {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TimingGuard {
    start: Instant,
    _session_id: String,
    _label: String,
}

impl TimingGuard {
    pub fn new(session_id: &str, label: &str) -> Self {
        Self {
            start: Instant::now(),
            _session_id: session_id.to_string(),
            _label: label.to_string(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_emitter_new() {
        let emitter = TelemetryEmitter::new();
        assert_eq!(emitter.event_count(), 0);
    }

    #[test]
    fn test_emit_tool_call_event() {
        let emitter = TelemetryEmitter::new();
        emitter.emit(TelemetryEvent::ToolCall {
            tool_name: "bash".to_string(),
            tool_call_id: "call-1".to_string(),
            duration_ms: 50,
            success: true,
            session_id: "sess-1".to_string(),
        });
        assert_eq!(emitter.event_count(), 1);
    }

    #[test]
    fn test_emit_context_event() {
        let emitter = TelemetryEmitter::new();
        emitter.emit(TelemetryEvent::ContextEvent {
            event_type: "compaction".to_string(),
            token_count: 1000,
            details: "compacted to 500 tokens".to_string(),
            session_id: "sess-1".to_string(),
        });
        assert_eq!(emitter.event_count(), 1);
    }

    #[test]
    fn test_emit_skill_retrieval() {
        let emitter = TelemetryEmitter::new();
        emitter.emit(TelemetryEvent::SkillRetrieval {
            query: "file operations".to_string(),
            matched_skills: vec!["fs_tool".to_string()],
            scores: vec![0.85],
            duration_ms: 10,
            session_id: "sess-1".to_string(),
        });
        assert_eq!(emitter.event_count(), 1);
    }

    #[test]
    fn test_emit_provider_call() {
        let emitter = TelemetryEmitter::new();
        emitter.emit(TelemetryEvent::ProviderCall {
            provider_name: "openai".to_string(),
            model: "gpt-4".to_string(),
            duration_ms: 1500,
            token_usage: Some((100, 50)),
            cached: false,
            session_id: "sess-1".to_string(),
        });
        assert_eq!(emitter.event_count(), 1);
    }

    #[test]
    fn test_emit_error() {
        let emitter = TelemetryEmitter::new();
        emitter.emit(TelemetryEvent::Error {
            component: "provider".to_string(),
            error_message: "connection refused".to_string(),
            session_id: "sess-1".to_string(),
        });
        assert_eq!(emitter.event_count(), 1);
    }

    #[test]
    fn test_timing_guard() {
        let guard = TimingGuard::new("sess-1", "test_operation");
        let elapsed = guard.elapsed_ms();
        assert!(elapsed < 100);
    }

    #[test]
    fn test_telemetry_emitter_multiple_events() {
        let emitter = TelemetryEmitter::new();
        emitter.emit(TelemetryEvent::ToolCall {
            tool_name: "read".to_string(),
            tool_call_id: "c1".to_string(),
            duration_ms: 10,
            success: true,
            session_id: "s1".to_string(),
        });
        emitter.emit(TelemetryEvent::ToolCall {
            tool_name: "write".to_string(),
            tool_call_id: "c2".to_string(),
            duration_ms: 20,
            success: true,
            session_id: "s1".to_string(),
        });
        assert_eq!(emitter.event_count(), 2);
    }
}
