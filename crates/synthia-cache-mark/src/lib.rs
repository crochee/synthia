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
}
