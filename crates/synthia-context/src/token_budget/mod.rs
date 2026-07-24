/// Token budget thresholds and status checking.
///
/// Provides configurable soft/hard limits with percentage-based alert levels
/// and absolute safety thresholds to prevent context window overflow.
mod budget;
mod safety;

pub use budget::*;
pub use safety::*;

#[cfg(test)]
mod tests;
