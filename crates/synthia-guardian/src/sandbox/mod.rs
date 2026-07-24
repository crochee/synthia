//! Sandbox isolation for tool execution.
//!
//! Provides command execution constraints including:
//! - Path validation (restrict to allowed paths)
//! - Command blacklisting (prevent dangerous operations)
//! - Timeout enforcement
//! - Output size limits
//!
//! All enforcement is deterministic and rule-based (P6: Distrust by Default).

mod config;
mod executor;
mod result;

pub use config::SandboxConfig;
pub use executor::SandboxExecutor;
pub use result::SandboxCheckResult;

#[cfg(test)]
mod tests;
