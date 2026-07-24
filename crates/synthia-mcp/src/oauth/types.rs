//! 4 data types for the OAuth 2.0 flow.
//!
//! - [`OAuthToken`] — the public token returned to
//!   callers, with `is_expired` / `is_expiring_soon`
//!   accessors.
//! - [`OAuthConfig`] — the per-server OAuth metadata
//!   (endpoints, client id, optional secret, redirect URI,
//!   scopes).
//! - [`AuthUrl`] — the result of the authorization-code
//!   flow's first leg: a URL the user must visit, plus the
//!   `state` value to verify on callback.
//! - `TokenResponse` (private) — the on-the-wire shape
//!   returned by the OAuth token endpoint, normalized
//!   to [`OAuthToken`] by [`super::flow::request_token`].

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub token_type: String,
    pub scope: Option<String>,
}

impl OAuthToken {
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() >= self.expires_at
    }

    pub fn is_expiring_soon(&self, buffer: std::time::Duration) -> bool {
        chrono::Utc::now() + buffer >= self.expires_at
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_uri: Option<String>,
    pub scopes: Vec<String>,
}

/// Response from the OAuth token endpoint.
#[derive(Debug, Deserialize)]
pub(super) struct TokenResponse {
    pub(super) access_token: String,
    pub(super) refresh_token: Option<String>,
    pub(super) expires_in: Option<u64>,
    pub(super) token_type: String,
    pub(super) scope: Option<String>,
}

/// Result of initiating the authorization code flow.
#[derive(Debug, Clone)]
pub struct AuthUrl {
    pub url: String,
    pub state: String,
}
