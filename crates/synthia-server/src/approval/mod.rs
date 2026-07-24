//! Server-side interactive approval service.
//!
//! Provides [`ApprovalState`] for tracking pending tool-call approvals,
//! [`HttpApprovalService`] as an HTTP-backed [`ApprovalService`], and
//! Axum handlers for the `/api/approvals` REST endpoints and the
//! `/ws/approvals` WebSocket stream.

pub mod routes;
pub mod service;
pub mod state;

pub use routes::{list_approvals, resolve_approval, ws_approvals_handler};
pub use service::HttpApprovalService;
pub use state::{
    ApprovalEvent,
    ApprovalListItem,
    ApprovalState,
    PendingApproval,
};
