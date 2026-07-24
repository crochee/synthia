//! 10 unit tests for the `oauth` module family.
//!
//! Coverage map:
//!
//! - [`super::store::CredentialStore`]: 5 tests
//!   (store_and_get / get_valid_token_for_expired /
//!   remove_token / persist_and_load /
//!   shutdown_persists_tokens).
//! - [`super::types::OAuthToken`]: 3 tests
//!   (expiry_check / expiring_soon).
//! - [`super::flow::CredentialStore`]: 2 tests
//!   (auth_code_flow_requires_config /
//!   get_or_refresh_token_no_token).

use super::*;

fn make_token(expires_in_secs: i64) -> OAuthToken {
    OAuthToken {
        access_token: "test-token".to_string(),
        refresh_token: Some("refresh-token".to_string()),
        expires_at: chrono::Utc::now()
            + chrono::Duration::seconds(expires_in_secs),
        token_type: "Bearer".to_string(),
        scope: Some("read write".to_string()),
    }
}

#[tokio::test]
async fn test_store_and_get_token() {
    let temp = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(temp.path().to_path_buf());
    let token = make_token(3600);
    store.store_token("test-server", token.clone()).await;
    let retrieved = store.get_token("test-server").await.unwrap();
    assert_eq!(retrieved.access_token, "test-token");
}

#[tokio::test]
async fn test_token_expiry_check() {
    let valid = make_token(3600);
    assert!(!valid.is_expired());

    let expired = make_token(-3600);
    assert!(expired.is_expired());
}

#[tokio::test]
async fn test_token_expiring_soon() {
    let token = make_token(30);
    assert!(token.is_expiring_soon(std::time::Duration::from_secs(60)));
    assert!(!token.is_expiring_soon(std::time::Duration::from_secs(10)));
}

#[tokio::test]
async fn test_get_valid_token_returns_none_for_expired() {
    let temp = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(temp.path().to_path_buf());
    store.store_token("expired-server", make_token(-3600)).await;
    assert!(store.get_valid_token("expired-server").await.is_none());
}

#[tokio::test]
async fn test_remove_token() {
    let temp = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(temp.path().to_path_buf());
    store.store_token("server1", make_token(3600)).await;
    store.remove_token("server1").await;
    assert!(store.get_token("server1").await.is_none());
}

#[tokio::test]
async fn test_persist_and_load() {
    let temp = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(temp.path().to_path_buf());
    store.store_token("server1", make_token(3600)).await;

    let store2 = CredentialStore::new(temp.path().to_path_buf());
    store2.load_from_disk().await.unwrap();
    assert!(store2.get_token("server1").await.is_some());
}

#[tokio::test]
async fn test_auth_code_flow_requires_config() {
    let temp = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(temp.path().to_path_buf());
    let result = store.initiate_auth_code_flow("unknown-server").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_or_refresh_token_no_token() {
    let temp = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(temp.path().to_path_buf());
    let result = store.get_or_refresh_token("unknown").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_shutdown_persists_tokens() {
    let temp = tempfile::tempdir().unwrap();
    let store = CredentialStore::new(temp.path().to_path_buf());
    store.store_token("server1", make_token(3600)).await;

    store.shutdown().await.unwrap();

    let store2 = CredentialStore::new(temp.path().to_path_buf());
    store2.load_from_disk().await.unwrap();
    assert!(store2.get_token("server1").await.is_some());
}
