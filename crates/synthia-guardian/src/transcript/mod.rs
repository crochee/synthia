//! Guardian 对话记录管理
//!
//! 此模块处理对话记录的收集、截断、审查提示构建和评估响应解析，
//! 用于 Guardian 审查过程。
//!
//! # Pipeline
//!
//! ```text
//! &[Message]                (raw conversation from provider)
//!   │
//!   ▼ collect_transcript_entries
//! Vec<TranscriptEntry>      (filtered, role-tagged entries)
//!   │
//!   ▼ build_review_prompt
//! String                    (LLM-facing review prompt with truncated
//!                            transcript, proposed action, retry reason)
//!   │                       (sent to GuardianReviewer)
//!   ▼ parse_assessment_response
//! Assessment                 (typed Guardian decision)
//! ```
//!
//! # Module Layout
//!
//! - [`types`]: The [`types::TranscriptEntry`] data struct (the unit of
//!   transcript content carried between pipeline stages).
//! - [`collect`]: [`collect::collect_transcript_entries`] turns raw
//!   `&[Message]` into `Vec<TranscriptEntry>`, filtering empty content.
//! - [`truncate`]: [`truncate::truncate_text`] is the byte-budget helper
//!   used by [`prompt::build_review_prompt`] to fit entries into the
//!   per-entry / per-message / per-tool token caps.
//! - [`prompt`]: [`prompt::build_review_prompt`] composes the full
//!   LLM-facing review prompt (transcript + action + retry reason +
//!   assessment schema).
//! - [`parse`]: [`parse::parse_assessment_response`] turns the LLM's
//!   free-form response text into a typed
//!   [`Assessment`](crate::review_types::Assessment), tolerating
//!   markdown code fences and surrounding prose.
//! - [`tests`]: All 28 unit tests covering collect, truncate, prompt,
//!   and parse paths.

mod collect;
mod parse;
mod prompt;
mod truncate;
mod types;

#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use collect::collect_transcript_entries;
pub use parse::parse_assessment_response;
pub use prompt::build_review_prompt;
pub use types::TranscriptEntry;
