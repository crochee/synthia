use synthia_session::store::SERVER_DEFAULT_USER_ID;

use super::*;
use crate::config::AuthConfig;

fn auth_with_keys(keys: &[&str]) -> AuthConfig {
    AuthConfig {
        enabled: true,
        api_keys: keys.iter().map(|s| s.to_string()).collect(),
        key_to_user: std::collections::HashMap::new(),
    }
}

#[test]
fn resolve_unmapped_key_uses_sha256_derivation() {
    let cfg = auth_with_keys(&["key-1"]);
    let uid = resolve_user_id_from_key("key-1", &cfg).expect("mapped");
    assert_eq!(uid.len(), 16);
    // Stable: same key → same user_id.
    let uid2 = resolve_user_id_from_key("key-1", &cfg).expect("mapped");
    assert_eq!(uid, uid2);
}

#[test]
fn resolve_explicit_map_wins_over_derivation() {
    let mut cfg = auth_with_keys(&["key-1"]);
    cfg.key_to_user
        .insert("key-1".to_string(), "team-alpha".to_string());
    let uid = resolve_user_id_from_key("key-1", &cfg).expect("mapped");
    assert_eq!(uid, "team-alpha");
}

#[test]
fn resolve_unknown_key_returns_none() {
    let cfg = auth_with_keys(&["key-1"]);
    assert!(resolve_user_id_from_key("key-2", &cfg).is_none());
}

#[test]
fn resolve_unconfigured_uses_server_default() {
    assert_eq!(resolve_user_id_unconfigured(), SERVER_DEFAULT_USER_ID);
}

#[test]
fn derive_user_id_is_lowercase_hex_16() {
    let id = derive_user_id("any-key");
    assert_eq!(id.len(), 16);
    assert!(
        id.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
}

#[test]
fn derive_user_id_distinguishes_keys() {
    assert_ne!(derive_user_id("a"), derive_user_id("b"));
}

#[test]
fn test_public_path_matching() {
    assert!(AuthMiddleware::<()>::is_public_path("/health"));
    assert!(!AuthMiddleware::<()>::is_public_path("/api/v1/chat"));
    assert!(!AuthMiddleware::<()>::is_public_path("/api/v1/skills"));
}

#[test]
fn test_path_traversal_bypass_prevention() {
    assert!(!AuthMiddleware::<()>::is_public_path(
        "/health/../api/v1/secret"
    ));
    assert!(!AuthMiddleware::<()>::is_public_path(
        "/health/./../private"
    ));
    assert!(!AuthMiddleware::<()>::is_public_path("/health//../secret"));
    assert!(!AuthMiddleware::<()>::is_public_path("/health/.."));
    assert!(!AuthMiddleware::<()>::is_public_path("/health/."));
}

#[test]
fn test_valid_public_subpaths() {
    assert!(AuthMiddleware::<()>::is_public_path("/health/check"));
    assert!(AuthMiddleware::<()>::is_public_path("/health/status"));
}

#[test]
fn test_path_traversal_blocked() {
    assert!(!is_public_path("/health/../api/v1/sessions"));
}

#[test]
fn test_url_encoded_traversal_blocked() {
    assert!(!is_public_path("/health/%2e%2e/api/v1/sessions"));
}

#[test]
fn test_exact_public_path_allowed() {
    assert!(is_public_path("/health"));
}

#[test]
fn test_non_public_path_blocked() {
    assert!(!is_public_path("/api/v1/sessions"));
}

fn is_public_path(path: &str) -> bool {
    AuthMiddleware::<()>::is_public_path(path)
}
