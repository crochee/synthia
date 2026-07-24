#[allow(clippy::module_inception)]
mod checker;
#[cfg(test)]
mod tests;

pub use checker::{PermissionChecker, Result};
// Test-only re-export so the checker tests can name `normalize_path` /
// `is_path_in_workspace` via `crate::checker::...` without making the
// private `checker` submodule public. Gated on `test` so the non-test
// lib build does not flag an unused re-export.
#[cfg(test)]
pub(crate) use checker::{is_path_in_workspace, normalize_path};
