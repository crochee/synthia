//! Summary generation with quality checking.

mod generator;
mod quality;

#[cfg(test)]
mod tests;

pub use generator::{create_summary_message, generate_summary};
pub(crate) use quality::check_summary_quality;