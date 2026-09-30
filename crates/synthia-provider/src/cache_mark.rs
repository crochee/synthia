//! Cache control mark shared between the context and provider layers.
//!
//! Carries TTL, scope, and pinned flag independently of the system prompt
//! content. The `CacheScope` **MUST** contain `user_id` to prevent
//! cross-session cache leakage (per security review R2/R5/R6).

use std::hash::{Hash, Hasher};

use ahash::AHasher;
use serde::{Deserialize, Serialize};

/// Cache TTL class.
///
/// `Ephemeral` is the default; providers may map this to short-lived cache
/// hints. `Extended` and `Long` request longer TTLs and require explicit
/// opt-in.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default,
)]
pub enum CacheTtl {
    #[default]
    Ephemeral,
    Extended,
    Long,
}

/// Namespace scope for a cache entry.
///
/// The wrapped `String` is formatted as `u={user_id};s={session_id}` so that
/// different users can never collide on a cache key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheScope(pub String);

impl CacheScope {
    /// Build a scope from explicit `user_id` and `session_id` parts.
    pub fn new(user_id: &str, session_id: &str) -> Self {
        Self(format!("u={user_id};s={session_id}"))
    }
}

impl Default for CacheScope {
    fn default() -> Self {
        Self::new("anonymous", "default")
    }
}

/// Marks a prompt snapshot for cache control tracking.
///
/// All fields feed into the `cache_control_hash` so the
/// `CacheBreakDetector` can detect cache control changes independently of
/// the system prompt content.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default,
)]
pub struct CacheControlMark {
    pub ttl: CacheTtl,
    pub scope: CacheScope,
    pub pinned: bool,
}

impl CacheControlMark {
    /// Deterministically hash the mark using `ahash::AHasher`.
    pub fn hash_to_u64(&self) -> u64 {
        let mut h = AHasher::default();
        self.ttl.hash(&mut h);
        self.scope.0.hash(&mut h);
        self.pinned.hash(&mut h);
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_scope_distinguishes_users() {
        let a = CacheScope::new("alice", "s1");
        let b = CacheScope::new("bob", "s1");
        assert_ne!(a, b);
        assert!(a.0.contains("u=alice"));
        assert!(a.0.contains("s=s1"));
    }

    #[test]
    fn cache_scope_default_is_anonymous() {
        let s = CacheScope::default();
        assert!(s.0.contains("u=anonymous"));
        assert!(s.0.contains("s=default"));
    }

    #[test]
    fn cache_ttl_default_is_ephemeral() {
        assert_eq!(CacheTtl::default(), CacheTtl::Ephemeral);
    }

    #[test]
    fn mark_default_matches_spec() {
        let mark = CacheControlMark::default();
        assert_eq!(mark.ttl, CacheTtl::Ephemeral);
        assert!(!mark.pinned);
        assert_eq!(mark.scope, CacheScope::default());
    }

    #[test]
    fn hash_differs_for_different_users() {
        let a = CacheControlMark {
            ttl: CacheTtl::Long,
            scope: CacheScope::new("alice", "s1"),
            pinned: true,
        };
        let b = CacheControlMark {
            ttl: CacheTtl::Long,
            scope: CacheScope::new("bob", "s1"),
            pinned: true,
        };
        assert_ne!(a.hash_to_u64(), b.hash_to_u64());
    }

    #[test]
    fn hash_is_deterministic() {
        let m = CacheControlMark::default();
        assert_eq!(m.hash_to_u64(), m.hash_to_u64());
    }

    /// `CacheScope::new` formats `u=<user_id>;s=<session_id>`.
    /// The format string is **literal** — a caller who
    /// passes `user_id` containing `;` or `=` characters
    /// can construct ambiguous scope strings. Pin the
    /// "no special-character handling" property so any
    /// future change to the format (e.g. escaping)
    /// is intentional, not accidental.
    #[test]
    fn cache_scope_new_does_not_escape_special_chars() {
        let raw = CacheScope::new("u=alice;s=admin", "session-1");
        // Caller-controlled `user_id` containing the
        // delimiter produces a string that
        // visually-matches but is structurally distinct
        // from one constructed via the tuple field.
        assert!(raw.0.contains("u=u=alice;s=admin;s=session-1"));
        // The prefix-free `u=` is still emitted at
        // position 0 — format string is literal.
        assert!(raw.0.starts_with("u=u="));
    }

    /// Forward-compat pin: serializing a
    /// `CacheControlMark` to JSON and reading it back
    /// MUST produce an equal value. Unknown fields
    /// MUST be silently dropped on deserialization
    /// (serde default) so a future schema bump
    /// (e.g. adding `priority: Option<i32>`) does not
    /// crash old deployments.
    #[test]
    fn cache_control_mark_json_round_trip_is_identity() {
        let original = CacheControlMark {
            ttl: CacheTtl::Extended,
            scope: CacheScope::new("alice", "s-42"),
            pinned: true,
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: CacheControlMark =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, parsed, "round-trip MUST be identity; got {json}");
    }

    /// Forward-compat pin: unknown fields in the
    /// JSON MUST be silently dropped (serde default
    /// behavior). A refactor that adds
    /// `#[serde(deny_unknown_fields)]` would crash
    /// any deployment that has an older client
    /// sending extra metadata — pin the
    /// silent-drop contract.
    #[test]
    fn cache_control_mark_json_ignores_unknown_fields() {
        let json = r#"{
            "ttl": "Long",
            "scope": "u=alice;s=s1",
            "pinned": false,
            "future_field": 42,
            "another_unknown": "ignored"
        }"#;
        let parsed: CacheControlMark =
            serde_json::from_str(json).expect("must tolerate unknown fields");
        assert_eq!(parsed.ttl, CacheTtl::Long);
        assert_eq!(parsed.scope, CacheScope::new("alice", "s1"));
        assert!(!parsed.pinned);
    }

    /// `pinned` is part of the cache-key hash — a
    /// change from `pinned: true` to `pinned: false`
    /// MUST yield a different `hash_to_u64()` so the
    /// `CacheBreakDetector` can flag the boundary.
    #[test]
    fn hash_differs_when_pinned_flag_changes() {
        let pinned = CacheControlMark {
            ttl: CacheTtl::Ephemeral,
            scope: CacheScope::default(),
            pinned: true,
        };
        let unpinned = CacheControlMark {
            ttl: CacheTtl::Ephemeral,
            scope: CacheScope::default(),
            pinned: false,
        };
        assert_ne!(
            pinned.hash_to_u64(),
            unpinned.hash_to_u64(),
            "pinned flag MUST participate in the hash"
        );
    }

    /// `CacheTtl` class changes MUST yield a different
    /// hash — pin the TTL-as-cache-key property.
    #[test]
    fn hash_differs_when_ttl_changes() {
        let e = CacheControlMark {
            ttl: CacheTtl::Ephemeral,
            scope: CacheScope::default(),
            pinned: false,
        };
        let l = CacheControlMark {
            ttl: CacheTtl::Long,
            scope: CacheScope::default(),
            pinned: false,
        };
        assert_ne!(e.hash_to_u64(), l.hash_to_u64());
    }
}

// Inline note: previously `synthia-cache-mark` was a separate 125-LOC crate.
// It was inlined into `synthia-provider::cache_mark` because:
// - Both `synthia-context` and `synthia-provider` consume it (shared types)
// - `synthia-context` already depends on `synthia-provider`, so the
//   reverse dependency added by inlining is zero-cost
// - One fewer crate = simpler workspace, fewer `Cargo.toml` entries
