//! Legacy re-export - canonical home is `synthia_core::cap_to_char_boundary`.
//!
//! Originally home to `cap_to_char_boundary`, this module is now a
//! 1-line shim so existing callers (`crate::builtin::utf8_safe::*`)
//! keep compiling. New callers should import from `synthia_core`
//! directly; the function has no tool-specific semantics.
pub use synthia_core::cap_to_char_boundary;
