//! Protection zone: determines which messages are safe to compact.

mod types;

#[cfg(test)]
mod tests;

pub use types::{CompactionBoundary, ProtectionZone};
