//! [`synthia_context`] — what fits in the next
//! prompt ([`ContextManager`]) and what is remembered between
//! prompts ([`Memory`]).
//!
//! Four context-manager strategies ship, all swappable at agent
//! construction time: [`NoopContextManager`],
//! [`TruncatingContextManager`] (the agent default),
//! [`SummarizingContextManager`], and [`DagContextManager`]. The
//! memory tier is layered — conversation / working / long-term —
//! with [`InMemoryMemory`], [`FileMemory`], and (behind the facade's
//! `sqlite` feature) `SqliteMemory`.

pub use synthia_context::*;
