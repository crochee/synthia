//! Synthia Guardian: Safety guardrails independent of LLM.
//!
//! This crate provides deterministic, rule-based protection mechanisms that
//! operate independently of LLM judgment, following P6 (Distrust by Default).
//!
//! # Components
//!
//! - **Permission Policy**: Per-tool permission levels (AutoApprove, RequireConfirm,
//!   RequireExplicit, Block)
//! - **Loop Detection**: Four-layer hash-based detection (GenericRepeat, PollNoProgress,
//!   PingPong, GlobalCircuit)
//! - **Circuit Breaker**: Tracks consecutive compaction failures
//! - **Injection Scanner**: Detects prompt injection attempts
//! - **Credential Guard**: Prevents credential leaks in output
//! - **Sandbox**: Command execution constraints
//! - **Guardian Review**: Security review system with approval requests, AI-based
//!   review via LLM, and transcript management

// Core review types (Assessment, Evidence, GuardianOption, ReviewDecision)
mod review_types;
pub use review_types::{Assessment, Evidence, GuardianOption, ReviewDecision};

// Guardian decision types (GuardianDecision, ActionType, Guardian trait)
mod guardian_decision;
pub use guardian_decision::{ActionType, GuardianDecision};

// Approval request types
mod approval_request;
pub use approval_request::{ApprovalRequest, McpAnnotations};

// Guardian configuration
pub mod config;
pub use config::{GuardianConfig, GuardianMode, GuardianRiskLevel};

// Guardian subagent policy system prompt
pub mod policy;
pub use policy::GUARDIAN_POLICY_PROMPT;

// Review logic and Guardian trait
pub mod review;
pub use review::{
    Guardian,
    RiskScore,
    SimpleGuardian,
    reviewer::GuardianReviewer,
};

// Guardian circuit breaker for denial tracking
mod guardian_circuit_breaker;
pub use guardian_circuit_breaker::GuardianCircuitBreaker;

// Guardian coordinator (hybrid layer)
mod guardian_coordinator;
pub use guardian_coordinator::{GuardianCheckOutcome, GuardianCoordinator};

// Guardian subagent reviewer (subagent-backed LLM review)
pub mod subagent_reviewer;
pub use subagent_reviewer::{
    GuardianSubagentError,
    GuardianSubagentFactory,
    GuardianSubagentOutput,
    GuardianSubagentReviewer,
    GuardianSubagentSpawnError,
};

// Self-reflection tool (LLM-callable Guardian review)
pub mod self_reflect;
pub use self_reflect::{
    SELF_REFLECT_TOOL_NAME,
    SelfReflectResult,
    run_self_reflect,
    self_reflect_tool_description,
    self_reflect_tool_parameters,
};

// Transcript management
pub mod transcript;
pub use transcript::{
    TranscriptEntry,
    build_review_prompt,
    collect_transcript_entries,
    parse_assessment_response,
};

// Existing safety guardrails
mod circuit_breaker;
mod credential_guard;
mod doom_loop_detector;
mod injection_scan;
mod loop_detector;
mod sandbox;
pub mod types;

// Public exports from existing modules
pub use circuit_breaker::CircuitBreaker;
pub use credential_guard::{CredentialGuard, CredentialMatch};
pub use doom_loop_detector::DoomLoopDetector;
pub use injection_scan::{InjectionMatch, InjectionScanner};
pub use loop_detector::LoopDetectorSet;
pub use sandbox::{SandboxCheckResult, SandboxConfig, SandboxExecutor};
pub use types::{
    GuardianState,
    LoopAction,
    LoopDetectionResult,
    LoopStatus,
    PermissionLevel,
    SecurityEvent,
    SecurityEventType,
    SecuritySeverity,
};
