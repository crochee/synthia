//! BM25-based skill matching.
//!
//! Three independent submodules expose a focused surface:
//!
//! - [`index`]: the inverted index itself —
//!   `BM25Index` plus the `Default` impl and all 6
//!   methods (`new`, `build`, `rebuild`, `search`,
//!   `add_skill`, `term_frequencies`).
//! - [`matcher`]: the registry-facing facade
//!   `BM25Matcher`, which takes a raw `&[Skill]` and a
//!   query and returns ranked `SkillMatch`es (the
//!   priority-bonus adjustment lives here).
//! - [`score`]: the small result type `SkillScore`
//!   produced by [`index::BM25Index::search`].
//!
//! The 18 unit tests live in [`tests`].

mod index;
mod matcher;
mod score;
#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests;

pub use index::BM25Index;
pub use matcher::BM25Matcher;
pub use score::SkillScore;
