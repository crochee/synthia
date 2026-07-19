// Legacy Tool trait usage during deprecation window (v3 toolification).
#![allow(deprecated)]

pub mod cli;
pub mod commands;
pub mod identity;
pub mod repl_core;
pub mod skill_cmd;
pub mod theme;
pub mod wire;
pub mod workspace;

pub use commands::*;
pub use identity::Identity;
pub use repl_core::*;
pub use wire::run_wire_client;
pub use workspace::*;
