//! Guardian 审批请求类型
//!
//! 定义可提交给 Guardian 进行安全审查的审批请求类型。
//!
//! # Module Layout
//!
//! - [`types`]: The [`ApprovalRequest`] enum (5 variants) and the
//!   [`McpAnnotations`](types::McpAnnotations) helper struct.
//! - [`accessors`]: The 5 `ApprovalRequest::{shell, exec_command,
//!   apply_patch, network_access, mcp_tool_call}` constructor methods
//!   plus the [`ApprovalRequest::id`] getter.
//! - [`serialization`]: The [`ApprovalRequest::to_json`] method
//!   (per-variant JSON representation for audit logs and Guardian prompts).
//! - [`summary`]: The [`ApprovalRequest::action_summary`] method
//!   (one-line human-readable summary for Guardian UI).
//! - [`tests`]: All 24 unit tests covering constructors, `id`, `to_json`,
//!   and `action_summary` behaviour.

mod accessors;
mod serialization;
mod summary;
mod types;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use types::{ApprovalRequest, McpAnnotations};
