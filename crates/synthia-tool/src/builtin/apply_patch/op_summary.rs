//! The [`op_summary`] helper — format one [`crate::builtin::v4a::PatchOp`]
//! as a short tag for tool output.
//!
//! - `*** Add File:` → `"A <path>"`
//! - `*** Update File:` → `"M <path>"`
//! - `*** Delete File:` → `"D <path>"`

use crate::builtin::v4a::PatchOp;

pub(super) fn op_summary(op: &PatchOp) -> String {
    match op {
        PatchOp::Add { path, .. } => format!("A {}", path.display()),
        PatchOp::Update { path, .. } => format!("M {}", path.display()),
        PatchOp::Delete { path } => format!("D {}", path.display()),
    }
}
