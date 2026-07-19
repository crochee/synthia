// Legacy Tool trait usage during deprecation window (v3 toolification).
#![allow(deprecated)]

pub mod fake_context;
pub mod fake_hook;
pub mod fake_memory;
pub mod fake_provider;
pub mod fake_tool;

pub use fake_context::*;
pub use fake_hook::*;
pub use fake_memory::*;
pub use fake_provider::*;
pub use fake_tool::*;
