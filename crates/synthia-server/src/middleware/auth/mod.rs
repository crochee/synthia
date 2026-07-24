//! V2 API authentication middleware.
//!
//! Checks for a Bearer token in the Authorization header against a configured API key.
//! Paths like /health and /api/v2/health bypass authentication.

pub mod layer;
pub mod middleware;
mod path;
pub mod types;
pub mod user_id;

#[cfg(test)]
mod tests;

pub use layer::AuthLayer;
pub use middleware::AuthMiddleware;
pub use types::RequestUserId;
pub use user_id::{
    derive_user_id,
    resolve_user_id_from_key,
    resolve_user_id_unconfigured,
};
