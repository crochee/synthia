//! Multi-Agent 模式层 — 基于 agent_as_tool() 和 SendMessage 的纯组合。
//!
//! 零新概念。所有模式都是 agent_as_tool() 的自然推论：
//! - Orchestrator: agents as tools, LLM picks whom
//! - GeneratorVerifier: gen + ver as tools, loop until PASS
//! - Workflow: pipe(agents as tools)
//! - Transfer: bidir SendMessage injection

pub mod generator_verifier;
#[cfg(test)]
mod integration_tests;
pub mod orchestrate;
pub mod transfer;
pub mod workflow;

pub use generator_verifier::GeneratorVerifier;
pub use orchestrate::{orchestrate, orchestrate_remote};
pub use transfer::transfer_bidirectional;
pub use workflow::Workflow;
