use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Represents a single LLM API call with its context and results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCallTrace {
    pub session_id: String,
    pub iteration: usize,
    pub model: String,
    pub message_count: usize,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    pub tool_calls: Vec<String>,
    pub latency_ms: u64,
    pub timestamp: DateTime<Utc>,
}

/// Pending trace data set by `start_trace`, completed by `end_trace`.
#[derive(Debug)]
struct PendingTrace {
    session_id: String,
    iteration: usize,
    model: String,
    message_count: usize,
}

/// Traces API calls in a bounded ring buffer, dropping the oldest when full.
///
/// Uses a two-phase recording approach: `start_trace` records the call context
/// (session, model, message count), and `end_trace` completes the trace with
/// results (tokens, tool calls, latency).
#[derive(Debug)]
pub struct ContextTracer {
    traces: Arc<Mutex<VecDeque<ApiCallTrace>>>,
    pending: Arc<Mutex<Option<PendingTrace>>>,
    max_traces: usize,
}

impl ContextTracer {
    /// Creates a new tracer that stores at most `max_traces` entries.
    pub fn new(max_traces: usize) -> Self {
        Self {
            traces: Arc::new(Mutex::new(VecDeque::with_capacity(max_traces))),
            pending: Arc::new(Mutex::new(None)),
            max_traces,
        }
    }

    /// Records the start of an API call, storing its context.
    /// Must be followed by `end_trace` to complete the trace.
    /// If a previous trace was started but not ended, it is discarded.
    pub fn start_trace(
        &self,
        session_id: &str,
        iteration: usize,
        model: &str,
        message_count: usize,
    ) {
        let mut pending = self.pending.lock().unwrap();
        *pending = Some(PendingTrace {
            session_id: session_id.to_string(),
            iteration,
            model: model.to_string(),
            message_count,
        });
    }

    /// Completes the most recently started trace by recording token counts,
    /// tool calls, and latency. The completed trace is added to the ring
    /// buffer, dropping the oldest if full. If no trace was started, this
    /// is a no-op.
    pub fn end_trace(
        &self,
        prompt_tokens: usize,
        completion_tokens: usize,
        tool_calls: Vec<String>,
        latency_ms: u64,
    ) {
        let pending = self.pending.lock().unwrap().take();
        let pending = match pending {
            Some(p) => p,
            None => return,
        };

        let trace = ApiCallTrace {
            session_id: pending.session_id,
            iteration: pending.iteration,
            model: pending.model,
            message_count: pending.message_count,
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
            tool_calls,
            latency_ms,
            timestamp: Utc::now(),
        };

        let mut traces = self.traces.lock().unwrap();
        if traces.len() >= self.max_traces {
            traces.pop_front();
        }
        traces.push_back(trace);
    }

    /// Returns a copy of all stored traces.
    pub fn get_traces(&self) -> Vec<ApiCallTrace> {
        self.traces.lock().unwrap().iter().cloned().collect()
    }

    /// Returns the number of stored traces.
    pub fn get_trace_count(&self) -> usize {
        self.traces.lock().unwrap().len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedContextTrace {
    pub stage: String,
    pub message_count: usize,
    pub total_tokens: usize,
    pub prefix_hash: String,
    pub timestamp: DateTime<Utc>,
}

impl DetailedContextTrace {
    pub fn new(
        stage: &str,
        message_count: usize,
        total_tokens: usize,
        prefix_hash: String,
    ) -> Self {
        Self {
            stage: stage.to_string(),
            message_count,
            total_tokens,
            prefix_hash,
            timestamp: Utc::now(),
        }
    }
}

pub fn compute_prefix_hash<S: AsRef<str>>(messages: &[S]) -> String {
    let mut hasher = Sha256::new();
    for msg in messages {
        hasher.update(msg.as_ref().as_bytes());
        hasher.update(b"\n");
    }
    let result = hasher.finalize();
    hex::encode(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_trace_new() {
        let trace =
            DetailedContextTrace::new("chat", 2, 50, "abc123".to_string());
        assert_eq!(trace.stage, "chat");
        assert_eq!(trace.message_count, 2);
        assert_eq!(trace.total_tokens, 50);
        assert_eq!(trace.prefix_hash, "abc123");
    }

    #[test]
    fn test_prefix_hash_deterministic() {
        let messages = vec!["system: test system", "user: test user"];
        let hash1 = compute_prefix_hash(&messages);
        let hash2 = compute_prefix_hash(&messages);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_prefix_hash_different_for_different_messages() {
        let messages1 = vec!["user: hello"];
        let messages2 = vec!["user: world"];
        let hash1 = compute_prefix_hash(&messages1);
        let hash2 = compute_prefix_hash(&messages2);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_prefix_hash_empty_messages() {
        let messages: Vec<&str> = vec![];
        let hash = compute_prefix_hash(&messages);
        assert!(!hash.is_empty());
    }

    #[test]
    fn test_context_trace_with_computed_hash() {
        let messages = vec!["system: You are helpful", "user: Hello"];
        let hash = compute_prefix_hash(&messages);
        let trace = DetailedContextTrace::new("chat", 2, 50, hash);
        assert!(!trace.prefix_hash.is_empty());
        assert_eq!(trace.prefix_hash.len(), 64);
    }

    #[test]
    fn test_context_tracer_new_empty() {
        let tracer = ContextTracer::new(10);
        assert_eq!(tracer.get_trace_count(), 0);
        assert!(tracer.get_traces().is_empty());
    }

    #[test]
    fn test_context_tracer_start_and_end_trace() {
        let tracer = ContextTracer::new(10);
        tracer.start_trace("session-1", 1, "gpt-4", 5);
        tracer.end_trace(100, 50, vec!["bash".to_string()], 200);
        assert_eq!(tracer.get_trace_count(), 1);

        let traces = tracer.get_traces();
        assert_eq!(traces[0].session_id, "session-1");
        assert_eq!(traces[0].iteration, 1);
        assert_eq!(traces[0].model, "gpt-4");
        assert_eq!(traces[0].message_count, 5);
        assert_eq!(traces[0].prompt_tokens, 100);
        assert_eq!(traces[0].completion_tokens, 50);
        assert_eq!(traces[0].total_tokens, 150);
        assert_eq!(traces[0].tool_calls, vec!["bash".to_string()]);
        assert_eq!(traces[0].latency_ms, 200);
    }

    #[test]
    fn test_context_tracer_end_trace_without_start_is_noop() {
        let tracer = ContextTracer::new(10);
        tracer.end_trace(100, 50, vec![], 100);
        assert_eq!(tracer.get_trace_count(), 0);
    }

    #[test]
    fn test_context_tracer_ring_buffer_drops_oldest() {
        let tracer = ContextTracer::new(3);

        tracer.start_trace("session-1", 1, "gpt-4", 5);
        tracer.end_trace(100, 50, vec![], 200);

        tracer.start_trace("session-2", 2, "gpt-4", 10);
        tracer.end_trace(200, 100, vec!["bash".to_string()], 300);

        tracer.start_trace("session-3", 3, "gpt-4", 15);
        tracer.end_trace(300, 150, vec!["read".to_string()], 400);

        assert_eq!(tracer.get_trace_count(), 3);

        // Adding a 4th trace should drop the first one
        tracer.start_trace("session-4", 4, "gpt-4", 20);
        tracer.end_trace(400, 200, vec![], 500);

        assert_eq!(tracer.get_trace_count(), 3);
        let traces = tracer.get_traces();
        assert_eq!(traces[0].session_id, "session-2");
        assert_eq!(traces[1].session_id, "session-3");
        assert_eq!(traces[2].session_id, "session-4");
    }

    #[test]
    fn test_context_tracer_multiple_traces_same_session() {
        let tracer = ContextTracer::new(5);

        for i in 1..=4 {
            tracer.start_trace("session-1", i, "gpt-4", i * 2);
            tracer.end_trace(i * 100, i * 50, vec![], (i * 100) as u64);
        }

        assert_eq!(tracer.get_trace_count(), 4);
        let traces = tracer.get_traces();
        assert_eq!(traces[0].iteration, 1);
        assert_eq!(traces[3].iteration, 4);
    }

    #[test]
    fn test_context_tracer_replaces_pending_trace() {
        let tracer = ContextTracer::new(5);

        tracer.start_trace("session-1", 1, "gpt-3.5", 3);
        tracer.start_trace("session-2", 2, "gpt-4", 5);
        tracer.end_trace(200, 100, vec!["bash".to_string()], 300);

        assert_eq!(tracer.get_trace_count(), 1);
        let traces = tracer.get_traces();
        assert_eq!(traces[0].session_id, "session-2");
        assert_eq!(traces[0].iteration, 2);
        assert_eq!(traces[0].model, "gpt-4");
    }
}
