//! Guardian 安全审查系统
//!
//! 此模块实现了 Guardian 审查系统，为潜在危险操作提供自动风险评估和审批。
//! Guardian 分析提议的操作，决定是自动批准（低风险）还是拒绝（高风险）。
//!
//! For detailed documentation, see [README.md](./README.md)

mod approval_request;
mod config;
mod review;
mod transcript;
mod types;

pub use approval_request::{ApprovalRequest, McpAnnotations};
pub use config::{GuardianConfig, GuardianMode, GuardianRiskLevel};
pub use review::{
    AdvancedGuardian,
    GUARDIAN_APPROVAL_RISK_THRESHOLD,
    Guardian,
    GuardianReviewer,
    RiskScore,
    SimpleGuardian,
};
pub use transcript::{
    TranscriptEntry,
    build_review_prompt,
    collect_transcript_entries,
    parse_assessment_response,
};
pub use types::{Assessment, Evidence, GuardianOption, ReviewDecision};
