use sha2::{Digest, Sha256};
use synthia_session::store::SERVER_DEFAULT_USER_ID;

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
