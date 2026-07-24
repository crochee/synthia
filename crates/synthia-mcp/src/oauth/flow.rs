//! The 4 auth-flow methods on
//! [`super::store::CredentialStore`] + the private
//! `request_token` HTTP helper +
//! [`CredentialStore::is_auth_error`].
//!
//! - [`CredentialStore::initiate_auth_code_flow`] —
//!   build the URL the user must visit to grant access.
//!   Uses the per-server [`OAuthConfig`] + the
//!   [`super::utils::generate_state`] random-state
//!   helper.
//! - [`CredentialStore::exchange_code_for_token`] —
//!   send `grant_type=authorization_code` + the code +
//!   the `state` returned by the user's callback to
//!   the token endpoint.
//! - [`CredentialStore::refresh_token`] —
//!   `grant_type=refresh_token`; preserves the old
//!   refresh token if the response omits one.
//! - [`CredentialStore::get_or_refresh_token`] —
//!   "give me a working token or refresh-then-give" with
//!   a 60s expiry buffer. Logs `error` and returns
//!   `Err(...)` on refresh failure.
//! - [`CredentialStore::is_auth_error`] — checks
//!   [`crate::types::McpError::is_auth_error_code`]
//!   for the standardized MCP codes -32001/-32002/-32003.

use super::{
    store::CredentialStore,
    types::{AuthUrl, OAuthToken},
    utils::{generate_state, url_encode},
};
use crate::jsonrpc::JsonRpcResponse;

impl CredentialStore {
    /// Initiate the OAuth 2.0 authorization code flow.
    /// Returns an authorization URL the user must visit to grant access.
    pub async fn initiate_auth_code_flow(
        &self,
        server_name: &str,
    ) -> Result<AuthUrl, String> {
        let config = self.get_config(server_name).await.ok_or_else(|| {
            format!("No OAuth config found for server '{}'", server_name)
        })?;

        let state = generate_state();
        let scopes_str = if !config.scopes.is_empty() {
            Some(config.scopes.join(" "))
        } else {
            None
        };

        let mut query_parts = Vec::new();
        query_parts.push(("response_type", "code"));
        query_parts.push(("client_id", config.client_id.as_str()));
        query_parts.push(("state", state.as_str()));

        if let Some(ref redirect_uri) = config.redirect_uri {
            query_parts.push(("redirect_uri", redirect_uri.as_str()));
        }

        if let Some(ref scopes) = scopes_str {
            query_parts.push(("scope", scopes.as_str()));
        }

        let query_string: String = query_parts
            .iter()
            .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        let separator = if config.authorization_endpoint.contains('?') {
            "&"
        } else {
            "?"
        };

        Ok(AuthUrl {
            url: format!(
                "{}{}{}",
                config.authorization_endpoint, separator, query_string
            ),
            state,
        })
    }

    /// Exchange an authorization code for an access token.
    pub async fn exchange_code_for_token(
        &self,
        server_name: &str,
        code: &str,
        state: &str,
    ) -> Result<OAuthToken, String> {
        let config = self.get_config(server_name).await.ok_or_else(|| {
            format!("No OAuth config found for server '{}'", server_name)
        })?;

        let mut params = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("state", state.to_string()),
        ];

        if let Some(ref redirect_uri) = config.redirect_uri {
            params.push(("redirect_uri", redirect_uri.clone()));
        }

        let token = self
            .request_token(
                &config.token_endpoint,
                &params,
                &config.client_secret,
            )
            .await?;

        self.store_token(server_name, token.clone()).await;
        Ok(token)
    }

    /// Refresh an expired token using the refresh token.
    pub async fn refresh_token(
        &self,
        server_name: &str,
    ) -> Result<OAuthToken, String> {
        let config = self.get_config(server_name).await.ok_or_else(|| {
            format!("No OAuth config found for server '{}'", server_name)
        })?;

        let current_token =
            self.get_token(server_name).await.ok_or_else(|| {
                format!("No token found for server '{}'", server_name)
            })?;

        let refresh_token = current_token
            .refresh_token
            .as_ref()
            .ok_or_else(|| {
                format!(
                    "No refresh token available for server '{}'",
                    server_name
                )
            })?
            .clone();

        let params = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token),
        ];

        let new_token = self
            .request_token(
                &config.token_endpoint,
                &params,
                &config.client_secret,
            )
            .await?;

        // Preserve the old refresh token if the response didn't include a new one
        let new_token = if new_token.refresh_token.is_none() {
            OAuthToken {
                refresh_token: current_token.refresh_token,
                ..new_token
            }
        } else {
            new_token
        };

        self.store_token(server_name, new_token.clone()).await;
        Ok(new_token)
    }

    /// Get a valid token, refreshing if expired.
    pub async fn get_or_refresh_token(
        &self,
        server_name: &str,
    ) -> Result<OAuthToken, String> {
        let buffer = std::time::Duration::from_secs(60);

        if let Some(token) = self.get_token(server_name).await {
            if !token.is_expiring_soon(buffer) {
                return Ok(token);
            }

            if token.refresh_token.is_some() {
                match self.refresh_token(server_name).await {
                    Ok(new_token) => return Ok(new_token),
                    Err(e) => {
                        tracing::error!(
                            server = %server_name,
                            error = %e,
                            "Token refresh failed"
                        );
                        return Err(format!(
                            "Token refresh failed for server '{}': {}",
                            server_name, e
                        ));
                    }
                }
            }
            return Ok(token);
        }

        Err(format!("No token found for server '{}'", server_name))
    }

    /// Check if a JSON-RPC response indicates an authentication error.
    /// Uses standardized MCP auth error codes: -32001 (auth), -32002 (expired), -32003 (forbidden).
    pub fn is_auth_error(response: &JsonRpcResponse) -> bool {
        if let Some(ref error) = response.error {
            return crate::types::McpError::is_auth_error_code(error.code);
        }
        false
    }

    pub(super) async fn request_token(
        &self,
        token_endpoint: &str,
        params: &[(&str, String)],
        client_secret: &Option<String>,
    ) -> Result<OAuthToken, String> {
        let form_params: Vec<(&str, &str)> =
            params.iter().map(|(k, v)| (*k, v.as_str())).collect();

        let mut request = self.http_client.post(token_endpoint);

        if let Some(secret) = client_secret {
            request = request.basic_auth("client", Some(secret));
        }

        let response = request
            .form(&form_params)
            .send()
            .await
            .map_err(|e| format!("Token request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<empty>".to_string());
            return Err(format!(
                "Token endpoint returned {}: {}",
                status, body
            ));
        }

        let token_response: super::types::TokenResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse token response: {}", e))?;

        let expires_in = token_response.expires_in.unwrap_or(3600);
        let expires_at =
            chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64);

        Ok(OAuthToken {
            access_token: token_response.access_token,
            refresh_token: token_response.refresh_token,
            expires_at,
            token_type: token_response.token_type,
            scope: token_response.scope,
        })
    }
}
