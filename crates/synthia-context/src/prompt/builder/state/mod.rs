//! State-management types used by [`super::PromptBuilder`].
//!
//! - [`SystemPromptPriority`]: ranking used when multiple prompt
//!   sources compete for the static slot.
//! - [`CacheStats`]: introspection for [`PromptState`] caches.
//! - [`PromptState`]: the actual section-result caches (global +
//!   session) keyed by section name and caching level.
//! - [`ResolvedPrompt`]: the output of `PromptBuilder::resolve`,
//!   holding the static + dynamic halves of the assembled prompt
//!   and the hashes used for cache-prefix stability.

mod prompt_state;
mod resolved_prompt;
mod system_prompt_priority;

pub use prompt_state::{CacheStats, PromptState};
pub use resolved_prompt::ResolvedPrompt;
pub use system_prompt_priority::SystemPromptPriority;
