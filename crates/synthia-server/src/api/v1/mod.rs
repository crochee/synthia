//! RESTful v1 API shared types: `List<T>`, `PageQuery`,
//! `TaskPageQuery`, cursor encode/decode, and request validation
//! helpers.
//!
//! These types are HTTP-wire concerns and live alongside the v1
//! error envelope (`error::ErrorCode` / `UserError`) in
//! `synthia-server::api`. Keeping the v1 surface in one place
//! makes the wire contract easy to audit.
//!
//! # Module Layout
//!
//! - [`list`]: [`list::List<T>`] — generic list envelope returned
//!   by every list endpoint.
//! - [`page_query`]: [`page_query::PageQuery`] and the
//!   resource-specific [`page_query::TaskPageQuery`] query
//!   parameter structs, plus the `DEFAULT_LIMIT` / `MAX_LIMIT`
//!   constants.
//! - [`cursor`]: opaque base64 (URL-safe, no-pad) cursor encoding
//!   for keyset pagination. Encodes the last resource ID of a
//!   page so the next page can resume via `WHERE id > last_id`.
//! - [`validation`]: request-parameter validation helpers —
//!   resource-name regex, sort whitelist enforcement, and API
//!   key masking.

pub mod cursor;
pub mod list;
pub mod page_query;
pub mod validation;

pub use cursor::{
    ResolvedPage,
    decode_cursor,
    encode_cursor,
    next_cursor,
    resolve_page,
};
pub use list::List;
pub use page_query::{DEFAULT_LIMIT, MAX_LIMIT, PageQuery, TaskPageQuery};
pub use validation::{api_key_mask, validate_resource_name, validate_sort};
