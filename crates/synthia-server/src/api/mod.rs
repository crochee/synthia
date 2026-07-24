//! Shared API utilities for `synthia-server` V2 endpoints.

pub mod envelope;
pub mod error;
pub mod pagination;
pub mod validation;

pub use envelope::{ApiResponse, json_data};
pub use error::{ApiError, ErrorDetail};
pub use pagination::{
    Cursor,
    Direction,
    PaginatedResponse,
    PaginationLinks,
    PaginationMeta,
    paginate_with_cursor,
};
pub use validation::{validate_content_not_empty, validate_priority};
