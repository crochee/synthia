//! [`synthia_macros`] — the derive macro that turns a
//! struct with an inherent `execute` method into a [`Tool`]
//! implementation (name, description, JSON Schema, and the async
//! `call` bridge).
//!
//! ```text
//! use synthia::macros::Tool;
//! ```
//!
//! The derive is named [`Tool`], the same short name as the trait; in
//! the prelude the name means the **trait** (documented there). Bring
//! this one in by module path when you want the macro.
//!
//! [`Tool`]: crate::tool::Tool

pub use synthia_macros::*;
