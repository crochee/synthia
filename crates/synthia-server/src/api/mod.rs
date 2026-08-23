//! HTTP-wire API namespace.
//!
//! Owns everything that touches the v1 wire contract:
//!
//! - [`v1`]: list response envelope, page query params, opaque
//!   cursor encoding, request-parameter validation helpers
//!   (resource-name regex, sort whitelist, API-key masking).
//!   `synthia_core::Error` — [`AppError`] (the boundary struct
//!   handlers return). The `From<synthia_core::Error> for
//!   AppError` impl owns the variant → StatusCode mapping inline;
//!   the server defines no error variants of its own.

pub mod error;
pub mod v1;
pub use error::{AppError, AppJson, AppPath, AppQuery};
pub use v1::{
    List,
    MAX_LIMIT,
    PageQuery,
    ResolvedPage,
    SessionPageQuery,
    api_key_mask,
    decode_cursor,
    encode_cursor,
    next_cursor,
    resolve_page,
    validate_resource_name,
    validate_sort,
};
