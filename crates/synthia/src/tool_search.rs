//! [`synthia_tool_search`] — the `search` tool: cross-domain discovery
//! over a host-built [`Registry`](crate::search::Registry).
//!
//! The [`search`](crate::search) feature ships the *building block* —
//! the engine keyed by domain, one `SearchEngine<T>` per catalogue the
//! host indexes. This module is its model-facing adapter: one
//! [`SearchTool`] that fans a query out across every engine in the
//! registry and returns the agent-facing projection (`domain` /
//! `id` / `title` / `why` / `score`, plus a `preview` when the
//! domain has content to show), so an agent can discover what is
//! available without the host hand-coding a tool per domain.
//!
//! ```rust,ignore
//! use std::sync::Arc;
//!
//! use synthia::prelude::*;
//! use synthia::search::{Registry, SearchEngine};
//! use synthia::tool_search::register_search_tool;
//!
//! // The host owns the catalogue: one engine per domain (built as the
//! // `synthia-search` docs show), one registry over all of them.
//! let registry = Arc::new(Registry::new());
//! registry.register(skill_engine); // built by the host
//! registry.register(memory_engine);
//!
//! let tools = ToolRegistry::new();
//! register_search_tool(&tools, Arc::clone(&registry));
//! ```
//!
//! `tool-search` is opt-in rather than default, exactly like `mcp` and
//! `tool-scheduler`: what the tool searches is a value only the host
//! can build, so the consumer opts in *and* wires the registry. The
//! tool is registered with `ToolExposure::Deferred` — the cold-start
//! tool list carries name + description and the real argument schema
//! is promoted on the first `search` call — so a model that never needs
//! cross-domain discovery never pays for the schema. Retrieval is
//! read-only and CPU-bound, so the tool runs in parallel with its
//! siblings.
//!
//! ```bash
//! cargo run --example search_tool_demo -p synthia-tool-search
//! ```

pub use synthia_tool_search::*;
