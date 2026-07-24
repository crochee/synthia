use std::{
    collections::{HashMap, VecDeque},
    sync::atomic::{AtomicU64, Ordering},
};

use sha2::{Digest, Sha256};
use synthia_provider::{Message, ToolDefinition};

/// KV Cache 前缀追踪器
///
/// 跟踪 system_prompt + tools_schema + messages_prefix 的 SHA-256 hash
/// 用于监控 prompt caching 命中率
pub struct PrefixTracker {
    /// 历史 hash 记录 (legacy history-based tracking)
    history: HashMap<String, u64>,
    /// 稳定计数器
    stable_count: AtomicU64,
    /// 变化计数器
    change_count: AtomicU64,
    /// Rolling window of (turn_id, hash) for stability ratio computation
    /// over the last `window_size` LLM calls.
    recent_window: VecDeque<(u64, String)>,
    /// Window size for `stability_ratio` computation (default 20).
    window_size: usize,
}

/// Default window size for stability ratio rolling window.
pub const DEFAULT_PREFIX_WINDOW: usize = 20;

impl PrefixTracker {
    pub fn new() -> Self {
        Self {
            history: HashMap::new(),
            stable_count: AtomicU64::new(0),
            change_count: AtomicU64::new(0),
            recent_window: VecDeque::with_capacity(DEFAULT_PREFIX_WINDOW),
            window_size: DEFAULT_PREFIX_WINDOW,
        }
    }

    /// Construct a tracker with a custom window size (used in tests).
    pub fn with_window(window_size: usize) -> Self {
        Self {
            history: HashMap::new(),
            stable_count: AtomicU64::new(0),
            change_count: AtomicU64::new(0),
            recent_window: VecDeque::with_capacity(window_size),
            window_size,
        }
    }

    /// 计算前缀 hash
    pub fn compute_prefix_hash(
        system_prompt: &str,
        skill_snapshot: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(system_prompt.as_bytes());
        hasher.update(skill_snapshot.as_bytes());
        let result = hasher.finalize();
        format!("{:x}", result)
    }

    /// Compute SHA-256 hash of the full prefix: system + tools + messages.
    ///
    /// The input is concatenated in a fixed order —
    /// `system_bytes || tools_schema_bytes || messages_prefix_bytes` —
    /// before hashing so the result is deterministic for the same prefix
    /// regardless of how the caller assembled the slices.
    ///
    /// Deterministic serialization is the caller's responsibility:
    /// - `system_bytes` — UTF-8 bytes of the assembled system prompt
    /// - `tools_schema_bytes` — canonical JSON (see
    ///   [`canonical_tools_schema_bytes`](Self::canonical_tools_schema_bytes))
    /// - `messages_prefix_bytes` — canonical JSON (see
    ///   [`canonical_messages_prefix_bytes`](Self::canonical_messages_prefix_bytes))
    pub fn compute_hash_bytes(
        system_bytes: &[u8],
        tools_schema_bytes: &[u8],
        messages_prefix_bytes: &[u8],
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(system_bytes);
        hasher.update(tools_schema_bytes);
        hasher.update(messages_prefix_bytes);
        format!("{:x}", hasher.finalize())
    }

    /// Serialize tool definitions as canonical JSON bytes for hashing.
    ///
    /// Tools are sorted by `name` so the hash is independent of input
    /// order. `serde_json` (without the `preserve_order` feature) uses
    /// `BTreeMap` for `Map<String, Value>`, giving deterministic key
    /// ordering for `input_schema`.
    pub fn canonical_tools_schema_bytes(tools: &[ToolDefinition]) -> Vec<u8> {
        let mut sorted: Vec<&ToolDefinition> = tools.iter().collect();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        serde_json::to_vec(&sorted).unwrap_or_default()
    }

    /// Serialize the messages prefix as canonical JSON bytes for hashing.
    ///
    /// The prefix is the cache-relevant portion: all messages from the
    /// start up to (but not including) the first message whose
    /// `tool_result_cleared_at` is set. Once a tool result is cleared
    /// (Stage 2 Hard Clear), the cache prefix is broken, so only the
    /// content *before* that point participates in the hash.
    pub fn canonical_messages_prefix_bytes(messages: &[Message]) -> Vec<u8> {
        let prefix: Vec<&Message> = messages
            .iter()
            .take_while(|m| m.tool_result_cleared_at.is_none())
            .collect();
        serde_json::to_vec(&prefix).unwrap_or_default()
    }

    /// 记录前缀使用 (legacy history-based; preserved for backward compat)
    pub fn record_prefix(&mut self, hash: &str) -> bool {
        if self.history.contains_key(hash) {
            self.stable_count.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            self.history
                .insert(hash.to_string(), self.history.len() as u64);
            self.change_count.fetch_add(1, Ordering::SeqCst);
            false
        }
    }

    /// Record a pre-LLM prefix snapshot. Pushes (turn_id, hash) into
    /// the rolling window, evicting the oldest entry when full.
    ///
    /// The hash covers the full prefix — `system_bytes ||
    /// tools_schema_bytes || messages_prefix_bytes` — so any change to
    /// the system prompt, tools schema, or messages prefix is detected.
    ///
    /// This is the entry point used by `StreamBuilder` to wire prefix
    /// tracking into the LLM call lifecycle. Returns the hash that was
    /// recorded, for symmetry with `record_post` callers.
    pub fn record_pre(
        &mut self,
        system_bytes: &[u8],
        tools_schema_bytes: &[u8],
        messages_prefix_bytes: &[u8],
        turn_id: u64,
    ) -> String {
        let hash = Self::compute_hash_bytes(
            system_bytes,
            tools_schema_bytes,
            messages_prefix_bytes,
        );
        self.recent_window.push_back((turn_id, hash.clone()));
        if self.recent_window.len() > self.window_size {
            self.recent_window.pop_front();
        }
        hash
    }

    /// Record a post-LLM prefix snapshot. Returns `true` if the
    /// post-call hash matches the most recent `record_pre` entry (stable)
    /// and `false` if the prefix changed mid-call.
    ///
    /// The caller MUST pass the same three slices used by the matching
    /// `record_pre` call — system, tools, and messages prefix — so the
    /// comparison is meaningful. Re-compute `messages_prefix_bytes` from
    /// the current message list to detect mid-call mutations; reuse the
    /// `system_bytes` and `tools_schema_bytes` captured before the call
    /// since neither should change during an LLM call.
    pub fn record_post(
        &mut self,
        system_bytes: &[u8],
        tools_schema_bytes: &[u8],
        messages_prefix_bytes: &[u8],
        _turn_id: u64,
    ) -> bool {
        let hash = Self::compute_hash_bytes(
            system_bytes,
            tools_schema_bytes,
            messages_prefix_bytes,
        );
        match self.recent_window.back() {
            Some((_, last_hash)) => *last_hash == hash,
            None => true, // no pre recorded = vacuously stable
        }
    }

    /// 获取稳定性比率 (legacy: stable/total over all-time history)
    pub fn stability_ratio(&self) -> f64 {
        let stable = self.stable_count.load(Ordering::SeqCst) as f64;
        let changes = self.change_count.load(Ordering::SeqCst) as f64;
        let total = stable + changes;
        if total > 0.0 { stable / total } else { 1.0 }
    }

    /// Windowed stability ratio over the last `window_size` LLM calls.
    ///
    /// Compares adjacent (turn_id, hash) entries: a "stable" pair means the
    /// hash did not change between consecutive LLM calls. The ratio is
    /// `stable_pairs / (window_len - 1)`. Empty window returns 1.0
    /// (vacuously stable).
    pub fn windowed_stability_ratio(&self) -> f64 {
        if self.recent_window.len() < 2 {
            return 1.0;
        }
        let entries: Vec<&(u64, String)> = self.recent_window.iter().collect();
        let mut stable = 0u64;
        for pair in entries.windows(2) {
            if pair[0].1 == pair[1].1 {
                stable += 1;
            }
        }
        let total = (entries.len() - 1) as f64;
        stable as f64 / total
    }

    /// Current size of the rolling window.
    pub fn window_len(&self) -> usize {
        self.recent_window.len()
    }

    /// Construct a telemetry event capturing current windowed stability.
    pub fn emit_stability_event(&self, turn_id: u64) -> PrefixStabilityEvent {
        PrefixStabilityEvent {
            turn_id,
            stability_ratio: self.windowed_stability_ratio(),
            recorded_at: std::time::SystemTime::now(),
        }
    }
}

impl Default for PrefixTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Telemetry event emitted by `PrefixTracker::emit_stability_event`.
#[derive(Debug, Clone, PartialEq)]
pub struct PrefixStabilityEvent {
    pub turn_id: u64,
    pub stability_ratio: f64,
    pub recorded_at: std::time::SystemTime,
}
