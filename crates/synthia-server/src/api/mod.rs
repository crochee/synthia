//! HTTP-wire API namespace.
//!
//! Owns everything that touches the v1 wire contract:
//!
//! - [`v1`]: list response envelope, page query params, opaque
//!   cursor encoding, request-parameter validation helpers
//!   (resource-name regex, sort whitelist, API-key masking).
//! - [`error`]: HTTP-wire error types — [`ErrorCode`] (stable
//!   snake_case classifier) and [`UserError`] (the
//!   `{code, message, result?}` envelope) with the
//!   `IntoResponse` impl. The V1 envelope
//!   (`{ error: { type, message } }`) emitted by
//!   `crate::error::ServerError` is a separate, older shape and
//!   remains untouched by this module.

pub mod error;
pub mod v1;

pub use error::{ErrorCode, UserError};
pub use v1::{
    DEFAULT_LIMIT,
    List,
    MAX_LIMIT,
    PageQuery,
    ResolvedPage,
    TaskPageQuery,
    api_key_mask,
    decode_cursor,
    encode_cursor,
    next_cursor,
    resolve_page,
    validate_resource_name,
    validate_sort,
};
