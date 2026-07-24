//! OAuth 2.0 support for MCP servers.
//!
//! - [`types`]: 4 data types
//!   ([`types::OAuthToken`] + [`types::OAuthConfig`] +
//!   [`types::AuthUrl`] + private `TokenResponse`).
//! - [`store`]: [`store::CredentialStore`] (per-server
//!   token / config map, JSON-file persistence under
//!   `<storage_path>/mcp_tokens.json`, graceful shutdown).
//! - [`flow`]: 4 authorization-code-flow methods on
//!   [`store::CredentialStore`]
//!   ([`flow::CredentialStore::initiate_auth_code_flow`] +
//!   [`flow::CredentialStore::exchange_code_for_token`] +
//!   [`flow::CredentialStore::refresh_token`] +
//!   [`flow::CredentialStore::get_or_refresh_token`]) plus
//!   [`flow::CredentialStore::is_auth_error`] (checks the
//!   standardized MCP codes -32001/-32002/-32003) and the
//!   private `request_token` HTTP helper.
//! - [`utils`]: 2 helper functions
//!   ([`utils::url_encode`] +
//!   [`utils::generate_state`]).
//! - [`tests`]: 10 unit tests covering token storage,
//!   expiry, refresh, persistence, and graceful shutdown.

mod flow;
mod store;
mod types;
mod utils;

#[cfg(test)]
mod tests;

pub use store::CredentialStore;
pub use types::{AuthUrl, OAuthConfig, OAuthToken};
