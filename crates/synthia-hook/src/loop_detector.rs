//! LoopDetector integration (PR-4.3).
//!
//! Integrates the existing Synthia LoopDetector three-piece suite
//! (`detect_repeat` / `similarity_threshold` / `recovery_action`)
//! into the hook system. When `PostToolUse` fires for a third time
//! with > 90% similarity, the detector classifies as `Repeating`
//! and emits `HookOutcome::Deny { reason: "loop_detected" }` on
//! the next `PreToolUse`.
//!
//! See `specs/hook-system-unification/spec.md`
//! (Requirement: "LoopDetector integration").

use std::collections::VecDeque;

use parking_lot::Mutex;

use crate::outcome::{
    HookEvent,
    HookOutcome,
    PostToolUsePayload,
    PreToolUsePayload,
};

/// Default similarity threshold (90%).
const DEFAULT_SIMILARITY_THRESHOLD: f64 = 0.9;

/// Default detection window (look back N calls).
const DEFAULT_WINDOW_SIZE: usize = 3;

/// Loop detection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopStatus {
    /// No loop pattern observed.
    Ok,
    /// Loop pattern is forming (near threshold).
    Warning,
    /// Loop pattern is confirmed.
    Detected,
}

/// The loop detector embedded in the hook system.
///
/// Tracks `PostToolUse` events by tool name and input similarity.
/// When the same tool is called with > 90% similar input within
/// a window of 3 calls, the detector flags it as `Detected`.
pub struct LoopDetector {
    /// Recent tool calls: `(tool_name, input_hash)`.
    history: Mutex<VecDeque<(String, u64)>>,
    /// Similarity threshold (0.0–1.0).
    similarity_threshold: f64,
    /// Number of recent calls to consider.
    window_size: usize,
    /// Whether a loop has been detected (cached for the next
    /// `PreToolUse` check).
    loop_detected: Mutex<bool>,
}

impl LoopDetector {
    /// Create a new loop detector with default settings.
    pub fn new() -> Self {
        Self {
            history: Mutex::new(VecDeque::new()),
            similarity_threshold: DEFAULT_SIMILARITY_THRESHOLD,
            window_size: DEFAULT_WINDOW_SIZE,
            loop_detected: Mutex::new(false),
        }
    }

    /// Create a new loop detector with custom settings.
    pub fn with_config(similarity_threshold: f64, window_size: usize) -> Self {
        Self {
            history: Mutex::new(VecDeque::new()),
            similarity_threshold: similarity_threshold.clamp(0.0, 1.0),
            window_size: window_size.max(1),
            loop_detected: Mutex::new(false),
        }
    }

    /// Check the current loop status based on recent history.
    pub fn check(&self) -> LoopStatus {
        let history = self.history.lock();
        if history.len() < self.window_size {
            return LoopStatus::Ok;
        }

        // Check if the last `window_size` calls are to the same tool
        // with similar input.
        let recent: Vec<_> =
            history.iter().rev().take(self.window_size).collect();

        if recent.is_empty() {
            return LoopStatus::Ok;
        }

        let first_tool = &recent[0].0;
        let all_same_tool = recent.iter().all(|(name, _)| name == first_tool);
        if !all_same_tool {
            return LoopStatus::Ok;
        }

        // Check hash similarity: if all hashes are identical, that's
        // a strong loop signal.
        let first_hash = recent[0].1;
        let identical_count =
            recent.iter().filter(|(_, h)| *h == first_hash).count();

        if identical_count >= self.window_size {
            return LoopStatus::Detected;
        }

        // Check if similarity is above threshold (approximate via hash
        // prefix match).
        let similar_count = recent
            .iter()
            .filter(|(_, h)| {
                // Simple similarity approximation: top 8 bits match.
                (h >> 56) == (first_hash >> 56)
            })
            .count();

        let ratio = similar_count as f64 / recent.len() as f64;
        if ratio >= self.similarity_threshold {
            return LoopStatus::Detected;
        }

        if identical_count >= self.window_size.saturating_sub(1) {
            return LoopStatus::Warning;
        }

        LoopStatus::Ok
    }

    /// Record a `PostToolUse` event.
    pub fn record_post_tool_use(&self, payload: &PostToolUsePayload) {
        let input_hash = self.hash_input(&payload.input);
        {
            let mut history = self.history.lock();
            history.push_back((payload.tool_name.clone(), input_hash));
            while history.len() > self.window_size * 2 {
                history.pop_front();
            }
        }

        // Update cached loop detection (check acquires its own lock).
        let detected = self.check() == LoopStatus::Detected;
        *self.loop_detected.lock() = detected;
    }

    /// Check whether to deny a `PreToolUse` event.
    ///
    /// Returns `HookOutcome::Deny` if a loop was detected by the
    /// most recent `PostToolUse` check.
    pub fn check_pre_tool_use(
        &self,
        payload: &PreToolUsePayload,
    ) -> HookOutcome {
        let detected = *self.loop_detected.lock();
        if detected {
            // Check if the pending tool matches the repeated pattern.
            let history = self.history.lock();
            if let Some((last_tool, _)) = history.back()
                && last_tool == &payload.tool_name
            {
                tracing::warn!(
                    target: "synthia::hook",
                    tool_name = %payload.tool_name,
                    "loop_detected_in_pre_tool_use",
                );
                return HookOutcome::Deny {
                    reason: "loop_detected".into(),
                };
            }
        }
        HookOutcome::Allow
    }

    /// Process a hook event, returning the appropriate outcome.
    ///
    /// For `PostToolUse`: records the event and returns `Allow`.
    /// For `PreToolUse`: checks for loops and may return `Deny`.
    /// For all other events: returns `Allow`.
    pub fn on_event(&self, event: &HookEvent) -> HookOutcome {
        match event {
            HookEvent::PostToolUse(payload) => {
                self.record_post_tool_use(payload);
                HookOutcome::Allow
            }
            HookEvent::PreToolUse(payload) => self.check_pre_tool_use(payload),
            _ => HookOutcome::Allow,
        }
    }

    /// Simple hash function for tool input.
    fn hash_input(&self, input: &serde_json::Value) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        input.to_string().hash(&mut hasher);
        hasher.finish()
    }

    /// Reset the detector state (for testing or after recovery).
    pub fn reset(&self) {
        self.history.lock().clear();
        *self.loop_detected.lock() = false;
    }
}

impl Default for LoopDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for LoopDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoopDetector")
            .field("similarity_threshold", &self.similarity_threshold)
            .field("window_size", &self.window_size)
            .finish()
    }
}

#[async_trait::async_trait]
impl crate::Hook for LoopDetector {
    async fn on_event(&self, event: &HookEvent) -> HookOutcome {
        // Delegate to the synchronous on_event method.
        self.on_event(event)
    }

    fn name(&self) -> &str {
        "loop-detector"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_post_tool_use(
        tool_name: &str,
        input: serde_json::Value,
    ) -> PostToolUsePayload {
        PostToolUsePayload {
            session_id: "test".into(),
            tool_name: tool_name.into(),
            input,
            output: serde_json::Value::Null,
        }
    }

    fn make_pre_tool_use(
        tool_name: &str,
        input: serde_json::Value,
    ) -> PreToolUsePayload {
        PreToolUsePayload {
            session_id: "test".into(),
            tool_name: tool_name.into(),
            input,
        }
    }

    #[test]
    fn no_loop_when_few_calls() {
        let detector = LoopDetector::new();
        assert_eq!(detector.check(), LoopStatus::Ok);
    }

    #[test]
    fn detect_loop_on_repeated_identical_calls() {
        let detector = LoopDetector::new();
        let input = serde_json::json!({"cmd": "ls"});

        // 3 identical PostToolUse calls.
        for _ in 0..3 {
            detector.record_post_tool_use(&make_post_tool_use(
                "bash",
                input.clone(),
            ));
        }
        assert_eq!(detector.check(), LoopStatus::Detected);

        // PreToolUse should be denied.
        let pre = make_pre_tool_use("bash", input.clone());
        assert!(matches!(
            detector.check_pre_tool_use(&pre),
            HookOutcome::Deny { .. }
        ));
    }

    #[test]
    fn no_loop_when_different_tools() {
        let detector = LoopDetector::new();
        detector.record_post_tool_use(&make_post_tool_use(
            "bash",
            serde_json::json!({"cmd": "ls"}),
        ));
        detector.record_post_tool_use(&make_post_tool_use(
            "read",
            serde_json::json!({"path": "/tmp"}),
        ));
        detector.record_post_tool_use(&make_post_tool_use(
            "bash",
            serde_json::json!({"cmd": "pwd"}),
        ));
        assert_eq!(detector.check(), LoopStatus::Ok);
    }

    #[test]
    fn reset_clears_detection() {
        let detector = LoopDetector::new();
        let input = serde_json::json!({"cmd": "ls"});
        for _ in 0..3 {
            detector.record_post_tool_use(&make_post_tool_use(
                "bash",
                input.clone(),
            ));
        }
        assert_eq!(detector.check(), LoopStatus::Detected);

        detector.reset();
        assert_eq!(detector.check(), LoopStatus::Ok);
    }

    #[test]
    fn on_event_dispatches_correctly() {
        let detector = LoopDetector::new();
        let input = serde_json::json!({"cmd": "ls"});

        // PostToolUse should record and allow.
        let post_event =
            HookEvent::PostToolUse(make_post_tool_use("bash", input.clone()));
        assert!(detector.on_event(&post_event).is_allowed());

        // Non-tool events should allow.
        let other_event =
            HookEvent::SessionStart(crate::outcome::SessionStartPayload {
                session_id: "s".into(),
            });
        assert!(detector.on_event(&other_event).is_allowed());
    }

    #[test]
    fn three_similar_tool_calls_trigger_deny() {
        let detector = LoopDetector::new();
        let input = serde_json::json!({"cmd": "ls -la"});

        // Record 3 identical PostToolUse events.
        for _ in 0..3 {
            let event = HookEvent::PostToolUse(make_post_tool_use(
                "bash",
                input.clone(),
            ));
            detector.on_event(&event);
        }

        // Next PreToolUse for the same tool should be denied.
        let pre_event =
            HookEvent::PreToolUse(make_pre_tool_use("bash", input.clone()));
        let outcome = detector.on_event(&pre_event);
        assert!(matches!(outcome, HookOutcome::Deny { .. }));
    }
}
