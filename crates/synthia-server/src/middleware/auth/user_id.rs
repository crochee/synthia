use sha2::{Digest, Sha256};
use synthia_session::manager::SERVER_DEFAULT_USER_ID;

use crate::config::AuthConfig;

/// Resolve a `user_id` from a request API key.
///
/// Resolution order:
/// 1. `auth_config.key_to_user` is consulted first; an explicit map
///    wins over derivation so operators can pin stable namespaces.
/// 2. If the key is in `auth_config.api_keys` but unmapped, derive
///    `user_id = hex(sha256(key))[..16]` — 64 bits, lowercase hex.
///    Stable per key; same key → same namespace; different keys →
///    different namespaces.
/// 3. If the key is unknown, return `None`. The caller should reject
///    the request (do not fall back to `LEGACY_USER_ID`; the §1
///    user_id namespace invariant requires a non-empty, key-bound
///    id).
///
/// When `auth_config` is empty (no API keys configured at all),
/// [`resolve_user_id_unconfigured`] is the right helper instead —
/// this function returns `None` in that case because there is no
/// key to resolve from.
pub fn resolve_user_id_from_key(
    api_key: &str,
    auth_config: &AuthConfig,
) -> Option<String> {
    // 1. Explicit map wins.
    if let Some(user_id) = auth_config.key_to_user.get(api_key) {
        return Some(user_id.clone());
    }

    // 2. Key is in api_keys but unmapped → derive.
    if auth_config.api_keys.iter().any(|k| k == api_key) {
        return Some(derive_user_id(api_key));
    }

    None
}

/// Resolve a `user_id` when no API key is configured on the server.
///
/// Returns the [`SERVER_DEFAULT_USER_ID`] placeholder so that
/// `SessionStore` operations still get a non-empty `user_id` and
/// the §1 invariant is preserved. Callers that need a fail-closed
/// posture (e.g. production agent runs) should reject at the
/// boundary; this helper exists to keep the unauthenticated dev
/// path functional.
pub fn resolve_user_id_unconfigured() -> String {
    SERVER_DEFAULT_USER_ID.to_string()
}

/// Derive a deterministic 16-hex-char user_id from an API key.
///
/// 64 bits of sha256 output, lowercase hex. The 64-bit namespace
/// is large enough to be collision-safe in the "one user per API
/// key" case (≈ 2^32 keys before 50% collision) while keeping
/// directory names compact.
pub fn derive_user_id(api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..8])
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::config::AuthConfig;

    fn empty_config() -> AuthConfig {
        AuthConfig {
            enabled: false,
            api_keys: vec![],
            key_to_user: HashMap::new(),
        }
    }

    // -- resolve_user_id_from_key: 3-case precedence -----------------

    /// Case 1: explicit key_to_user map
    /// wins over api_keys derivation. Even
    /// if a key is in api_keys, if it also
    /// has an explicit mapping, that
    /// mapping wins.
    #[test]
    fn resolve_user_id_from_key_explicit_map_wins() {
        let mut key_to_user = HashMap::new();
        key_to_user.insert("my-key".to_string(), "pinned-user".to_string());
        let cfg = AuthConfig {
            enabled: true,
            api_keys: vec!["my-key".to_string()],
            key_to_user,
        };
        assert_eq!(
            resolve_user_id_from_key("my-key", &cfg),
            Some("pinned-user".to_string())
        );
    }

    /// Case 2: key in api_keys but unmapped
    /// → derive user_id (64-bit hex).
    #[test]
    fn resolve_user_id_from_key_known_key_derives_user_id() {
        let cfg = AuthConfig {
            enabled: true,
            api_keys: vec!["known-key".to_string()],
            key_to_user: HashMap::new(),
        };
        let user_id = resolve_user_id_from_key("known-key", &cfg)
            .expect("known key MUST yield a user_id");
        // Derived MUST match direct
        // `derive_user_id()` call.
        assert_eq!(user_id, derive_user_id("known-key"));
    }

    /// Case 3: key unknown → None
    /// (fail-closed at auth boundary; do
    /// NOT fall back to
    /// SERVER_DEFAULT_USER_ID).
    #[test]
    fn resolve_user_id_from_key_unknown_key_returns_none() {
        let cfg = AuthConfig {
            enabled: true,
            api_keys: vec!["known-key".to_string()],
            key_to_user: HashMap::new(),
        };
        assert_eq!(resolve_user_id_from_key("unknown-key", &cfg), None);
    }

    /// Empty config (no api_keys at all) →
    /// None for any key. (Use
    /// `resolve_user_id_unconfigured` for
    /// the unauthenticated path.)
    #[test]
    fn resolve_user_id_from_key_empty_config_returns_none() {
        let cfg = empty_config();
        assert_eq!(resolve_user_id_from_key("any", &cfg), None);
    }

    // -- resolve_user_id_unconfigured ---------------------------------

    /// When the server has no API keys
    /// configured, the helper MUST return
    /// SERVER_DEFAULT_USER_ID verbatim.
    #[test]
    fn resolve_user_id_unconfigured_returns_server_default_user_id() {
        assert_eq!(
            resolve_user_id_unconfigured(),
            SERVER_DEFAULT_USER_ID.to_string()
        );
        assert_eq!(resolve_user_id_unconfigured(), "dev");
    }

    // -- derive_user_id ----------------------------------------------

    /// `derive_user_id` MUST produce
    /// exactly 16 hex characters (8 bytes
    /// truncated).
    #[test]
    fn derive_user_id_is_exactly_sixteen_hex_chars() {
        let uid = derive_user_id("any-key");
        assert_eq!(uid.len(), 16);
        // All chars MUST be lowercase hex.
        assert!(
            uid.chars().all(|c| c.is_ascii_hexdigit()),
            "uid must be hex: {uid}"
        );
        // No uppercase letters (Rust hex
        // defaults to lowercase via
        // hex::encode).
        assert!(
            uid.chars().all(|c| !c.is_ascii_uppercase()),
            "uid must be lowercase: {uid}"
        );
    }

    /// `derive_user_id` MUST be
    /// deterministic — same input
    /// produces the same output across
    /// multiple calls.
    #[test]
    fn derive_user_id_is_deterministic() {
        let a = derive_user_id("stable-key");
        let b = derive_user_id("stable-key");
        let c = derive_user_id("stable-key");
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    /// `derive_user_id` MUST produce
    /// distinct outputs for distinct
    /// inputs (collision-resistance
    /// sanity check).
    #[test]
    fn derive_user_id_distinct_keys_yield_distinct_user_ids() {
        let a = derive_user_id("key-a");
        let b = derive_user_id("key-b");
        let c = derive_user_id("key-c");
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }

    /// `derive_user_id("")` MUST produce a
    /// deterministic 16-char output — the
    /// empty string is a valid input (not
    /// an error).
    #[test]
    fn derive_user_id_empty_string_still_returns_16_hex_chars() {
        let uid = derive_user_id("");
        assert_eq!(uid.len(), 16);
        assert!(uid.chars().all(|c| c.is_ascii_hexdigit()));
        // Empty input MUST be deterministic.
        assert_eq!(uid, derive_user_id(""));
    }
}
