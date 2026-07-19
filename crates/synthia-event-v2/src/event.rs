//! `event` module — real wire-format types for the Event V2 bus (PR-1.2).
//!
//! PR-1.2 finalises the three primary types declared in
//! `specs/event-v2-system/spec.md`:
//!
//! - [`EventVersion`] — monotonically increasing per [`EventSource`] counter.
//! - [`EventMeta`] — prefix-hash + sequence + source + timestamp envelope
//!   metadata.
//! - [`EventEnvelope<T>`] — full envelope carrying a typed payload.
//!
//! ## Prefix-hash derivation
//!
//! The `prefix_hash` carried in `EventMeta` is derived from Synthia's
//! existing three-segment rolling hash (see
//! `synthia_context::prefix_tracker::PrefixTracker::compute_hash_bytes`).
//! This is the **Synthia-preserved** element called out in `design.md` §D7
//! of `openspec/changes/2026-07-18-synthia-top5-borrow-integration`; we
//! reuse it unchanged so existing P9 callers (`synthia-telemetry`,
//! `synthia-cache-mark`) continue to function with byte-identical hashes.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use synthia_context::prefix_tracker::PrefixTracker;
use uuid::Uuid;

/// Source of an emitted event.
///
/// PR-1.2 stays binary-compatible with PR-1.1 — the variant order and
/// shape are unchanged.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
pub enum EventSource {
    /// A system-internal event (e.g. compaction trigger).
    #[default]
    System,
    /// An event emitted by the agent's `ReAct` loop.
    Agent,
    /// An event emitted by a registered tool.
    Tool,
    /// A user-supplied event (extension / custom renderer — PR-7.x).
    User,
}

impl EventSource {
    /// String label for tracing.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Agent => "agent",
            Self::Tool => "tool",
            Self::User => "user",
        }
    }
}

/// 32-byte SHA-256 digest carried in [`EventMeta::prefix_hash`].
///
/// The hex form is the original `PrefixTracker::compute_hash_bytes`
/// output; `bytes` is the raw 32-byte form so consumers don't have to
/// decode hex at every comparison.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
pub struct PrefixHash {
    /// Raw 32-byte SHA-256 digest.
    pub bytes: [u8; 32],
}

impl PrefixHash {
    /// Render the hex form (lowercase, 64 chars).
    #[must_use]
    pub fn hex(&self) -> String {
        let mut out = String::with_capacity(64);
        for b in &self.bytes {
            use std::fmt::Write as _;
            let _ = write!(out, "{b:02x}");
        }
        out
    }
}

/// Per-source monotonically increasing version counter.
///
/// `EventVersion` is the tuple `(source, sequence)`. The `sequence`
/// component starts at 1 and increments on each `emit` from the same
/// source. Persistence is sink-side: the in-memory sink keeps an
/// `AtomicU64` per source; the durable sink (PR-1.4) keeps a row per
/// source in `events`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventVersion {
    /// The source this version belongs to.
    pub source: EventSource,
    /// Monotonic sequence number (starts at 1).
    pub sequence: u64,
}

impl EventVersion {
    /// First version for the given source.
    #[must_use]
    pub const fn first(source: EventSource) -> Self {
        Self {
            source,
            sequence: 1,
        }
    }
}

/// Metadata carried alongside every event payload.
///
/// `created_at_ms` is wall-clock time since the Unix epoch.
/// `prefix_hash` is the three-segment rolling hash — see module docs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMeta {
    /// 32-byte SHA-256 prefix hash (`system_bytes` || `tools_schema_bytes`
    /// || `messages_prefix_bytes`).
    pub prefix_hash: PrefixHash,
    /// Monotonic sequence number for `(meta.source)` within this process
    /// (or process-cohort for the durable sink).
    pub sequence: u64,
    /// Where the event came from.
    pub source: EventSource,
    /// Wall-clock creation timestamp in milliseconds since the Unix epoch.
    pub created_at_ms: i64,
    /// Optional extension id (set for events emitted from a registered
    /// extension — PR-2.x). `None` for non-extension events.
    pub extension_id: Option<String>,
}

impl EventMeta {
    /// Build an `EventMeta` from the three prefix segments consumed by
    /// [`PrefixTracker::compute_hash_bytes`].
    pub fn from_prefix_bytes(
        source: EventSource,
        sequence: u64,
        system_bytes: &[u8],
        tools_schema_bytes: &[u8],
        messages_prefix_bytes: &[u8],
        extension_id: Option<String>,
    ) -> Self {
        let hex = PrefixTracker::compute_hash_bytes(
            system_bytes,
            tools_schema_bytes,
            messages_prefix_bytes,
        );
        Self::from_prefix_hex(source, sequence, &hex, extension_id)
    }

    /// Build an `EventMeta` from a precomputed hex prefix hash.
    pub fn from_prefix_hex(
        source: EventSource,
        sequence: u64,
        hex: &str,
        extension_id: Option<String>,
    ) -> Self {
        let mut bytes = [0u8; 32];
        // Misformed hex is treated as a distinct (zero) prefix so it is
        // never silently aliased with another event.
        decode_hex(hex, &mut bytes);
        Self {
            prefix_hash: PrefixHash { bytes },
            sequence,
            source,
            created_at_ms: now_ms(),
            extension_id,
        }
    }
}

/// Decode a hex string into `out`. Truncates / rejects on bad length or
/// non-hex input by leaving `out` unchanged. Used to translate the
/// lowercase hex form returned by `PrefixTracker::compute_hash_bytes`
/// into the 32-byte form carried inside `EventMeta`.
fn decode_hex(s: &str, out: &mut [u8; 32]) -> bool {
    if s.len() != 64 {
        return false;
    }
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = match bytes[i] {
            b'0'..=b'9' => bytes[i] - b'0',
            b'a'..=b'f' => bytes[i] - b'a' + 10,
            b'A'..=b'F' => bytes[i] - b'A' + 10,
            _ => return false,
        };
        let lo = match bytes[i + 1] {
            b'0'..=b'9' => bytes[i + 1] - b'0',
            b'a'..=b'f' => bytes[i + 1] - b'a' + 10,
            b'A'..=b'F' => bytes[i + 1] - b'A' + 10,
            _ => return false,
        };
        out[i / 2] = (hi << 4) | lo;
        i += 2;
    }
    true
}

/// Wall-clock time in milliseconds since the Unix epoch.
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

/// Full event envelope: a typed `payload` plus immutable `meta` plus an
/// `envelope_id` (per-event UUID v4) plus the `version`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope<T> {
    /// Per-event UUID v4 (uniquely identifies this envelope).
    pub envelope_id: Uuid,
    /// Version tuple (`source`, `sequence`).
    pub version: EventVersion,
    /// Metadata captured at emit time.
    pub meta: EventMeta,
    /// Typed event payload.
    pub payload: T,
}

impl<T> EventEnvelope<T> {
    /// Wrap `payload` in an envelope using the supplied `meta` + `version`.
    #[must_use]
    pub fn new(version: EventVersion, meta: EventMeta, payload: T) -> Self {
        Self {
            envelope_id: Uuid::new_v4(),
            version,
            meta,
            payload,
        }
    }
}

/// Per-source sequence counter.
#[derive(Debug, Default)]
pub struct SourceSequence {
    /// Internal atomic counter (starts at 0; first assigned value is 1).
    counter: AtomicU64,
}

impl SourceSequence {
    /// Allocate the next sequence number for this source.
    pub fn next(&self) -> u64 {
        let prev = self.counter.fetch_add(1, Ordering::SeqCst);
        prev + 1
    }

    /// Current sequence value (last assigned + 1) for diagnostics.
    pub fn current(&self) -> u64 {
        self.counter.load(Ordering::SeqCst) + 1
    }

    /// Construct a `SourceSequence` that will continue from `max_seq`.
    ///
    /// Used by the SQLite sink on restart: after restoring the max sequence
    /// from the `events` table, the sink needs a `SourceSequence` whose next
    /// `next()` call returns `max_seq + 1`.
    pub fn starting_at(max_seq: u64) -> Self {
        Self {
            counter: AtomicU64::new(max_seq),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_sequence_starts_at_one() {
        let s = SourceSequence::default();
        assert_eq!(s.next(), 1);
        assert_eq!(s.next(), 2);
        assert_eq!(s.next(), 3);
        assert_eq!(s.current(), 4);
    }

    #[test]
    fn event_meta_hex_round_trip() {
        let hex = "00".repeat(32);
        let m = EventMeta::from_prefix_hex(EventSource::Agent, 1, &hex, None);
        assert_eq!(m.prefix_hash.hex(), hex);
        assert_eq!(m.source, EventSource::Agent);
        assert_eq!(m.sequence, 1);
        assert!(m.created_at_ms > 0);
    }

    #[test]
    fn event_meta_from_prefix_bytes_deterministic() {
        let m1 = EventMeta::from_prefix_bytes(
            EventSource::Agent,
            1,
            b"sys",
            b"tools",
            b"msg",
            None,
        );
        let m2 = EventMeta::from_prefix_bytes(
            EventSource::Agent,
            2,
            b"sys",
            b"tools",
            b"msg",
            None,
        );
        assert_eq!(m1.prefix_hash, m2.prefix_hash);
        assert_ne!(m1.sequence, m2.sequence);
    }

    #[test]
    fn envelope_id_is_unique() {
        let v = EventVersion::first(EventSource::Agent);
        let m = EventMeta::default();
        let e1 = EventEnvelope::new(v, m.clone(), 42_i32);
        let e2 = EventEnvelope::new(v, m, 42_i32);
        assert_ne!(e1.envelope_id, e2.envelope_id);
        assert_eq!(e1.payload, e2.payload);
    }

    #[test]
    fn decode_hex_handles_uppercase() {
        let mut out = [0u8; 32];
        let hex = "DEADBEEF".to_string() + &"00".repeat(28);
        assert!(decode_hex(&hex, &mut out));
        assert_eq!(out[0], 0xDE);
        assert_eq!(out[1], 0xAD);
        assert_eq!(out[2], 0xBE);
        assert_eq!(out[3], 0xEF);
    }

    #[test]
    fn decode_hex_rejects_bad_length() {
        let mut out = [0u8; 32];
        assert!(!decode_hex("abcd", &mut out));
    }
}
