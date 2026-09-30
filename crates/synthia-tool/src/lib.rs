//! # synthia-tool
//!
//! Tools: what an agent may call, what it is *told* it may call, and
//! what a call is allowed to touch.
//!
//! A tool is a name, a JSON Schema, and an async `call`. Everything else
//! in this crate is about the three questions a deployment actually
//! asks, and they are deliberately three different answers:
//!
//! | question | mechanism |
//! |---|---|
//! | what is **registered** | [`ToolRegistry`] — name → entries, last registration wins |
//! | what the model is **shown** | [`ToolExposure`] (`Direct` / `Deferred` / `Hidden`), the [`surface`] projection, and the read-side wrappers [`AdaptiveRegistry`] / [`GroupedRegistry`] / [`RestrictedRegistry`] |
//! | what may actually **run** | the registry's privacy flag ([`ToolRegistry::set_hidden`] refuses dispatch), plus whatever boundary the tool itself enforces — [`workspace`] path confinement here, and each plugin's own policy (the shell brick owns process confinement) |
//!
//! Visibility ≠ executability is the load-bearing decision: a tool in an
//! inactive group, or above a small model's tier cap, disappears from
//! the model's tool list **without** leaving the registry — a skill, a
//! workflow step, or a subagent that needs it still executes it. The
//! split costs one word in the docs and saves every consumer from
//! re-registering tools as a deployment's surface changes.
//!
//! ## Assemble a tool surface
//!
//! ```rust
//! use std::sync::Arc;
//!
//! use synthia_tool::{Tool, ToolEntry, ToolOutput, ToolRegistry};
//!
//! struct WordCount;
//!
//! #[async_trait::async_trait]
//! impl Tool for WordCount {
//!     fn name(&self) -> &str {
//!         "word_count"
//!     }
//!
//!     fn description(&self) -> &str {
//!         "Count the words in `text`."
//!     }
//!
//!     fn parameters(&self) -> serde_json::Value {
//!         serde_json::json!({
//!             "type": "object",
//!             "properties": { "text": { "type": "string" } },
//!             "required": ["text"],
//!         })
//!     }
//!
//!     async fn call(
//!         &self,
//!         input: serde_json::Value,
//!         _context: &synthia_tool::Context,
//!     ) -> ToolOutput {
//!         let text = input["text"].as_str().unwrap_or_default();
//!         ToolOutput::text(format!(
//!             "{} words",
//!             text.split_whitespace().count()
//!         ))
//!     }
//! }
//!
//! let registry = ToolRegistry::new();
//! let custom = Arc::new(WordCount);
//! registry.register_entry(ToolEntry::new(custom.clone()));
//!
//! // Registered …
//! assert!(registry.snapshot().iter().any(|t| t.name == "word_count"));
//! // … and advertised to the model as a schema it can call.
//! let advertised = synthia_tool::project_tool_definitions(
//!     &registry.descriptors(),
//!     &Default::default(),
//!     None,
//! );
//! assert!(advertised.iter().any(|d| d.name == "word_count"));
//! # let _ = custom;
//! ```
//!
//! ## What is here
//!
//! - [`Tool`] / [`ToolEntry`] — the trait and its registration wrapper
//!   (provenance, exposure, privacy flag, execution mode).
//! - The **agent-facing tool implementations are separate plugin
//!   crates** — `synthia-tool-read` / `-write` / `-shell` / `-todo` /
//!   `-web` (and `-task` / `-scheduler` for the two that adapt a crate
//!   behind them) each carry exactly one tool and depend only on this
//!   crate plus, for those two, the crate they adapt. Compose a
//!   registry by registering the bricks you want;
//!   this crate ships no default set. What it does carry beyond the
//!   paradigm is two *synthetic contracts* the harness itself needs
//!   ([`full_output`]'s retrieval tool and [`structured_output`]'s
//!   schema-capture tool, auto-injected by `ReActAgent::with_output_schema`)
//!   plus the registry's `ToolEntry::dynamic` passthrough.
//! - [`surface`] — the projection that turns the registry into the wire
//!   `ToolDefinition`s, including deferred schemas.
//! - [`output`] / [`full_output`] — the render contract, and the
//!   stash-then-fetch path for results too large to commit.
//! - [`workspace`] — path confinement for file-touching tools. A
//!   boundary that needs more than a path — process confinement, an
//!   approval gate, cross-call serialisation — belongs to the plugin
//!   that enforces it: `synthia-tool-shell` carries the OS execution
//!   policy, and a tool that mutates files declares
//!   [`ExecutionMode::Sequential`] so the loop never races it with a
//!   sibling call.
//! - `spawn` — the [`synthia_core::spawn::Spawner`] seam dispatch
//!   detaches onto.
//!
//! ## What is deliberately *not* here
//!
//! - **No runtime choice.** Dispatch detaches through
//!   [`synthia_core::spawn::Spawner`]; the plugin tool crates
//!   (`synthia-tool-read` / `-write` / `-shell` / `-todo` / `-web`
//!   / `-task`) use `tokio::fs` / `tokio::process`, and a consumer
//!   on another runtime registers their own tools.
//!   [`synthia_core::spawn`] documents the boundary.
//!
//! - **No retries.** A tool failure is a result the model reads
//!   ([`ToolOutput`]), not a transport error to retry.
//!
//! See [`MINIMAL.md`](https://github.com/) for the smallest complete
//! agent and `SEAMS.md` for every swappable piece.

pub mod adaptive;
pub mod full_output;
pub mod grouped;
pub mod output;
pub mod registry;
pub mod restriction;
pub mod schema_builder;
mod spawn;
pub mod structured_output;
pub mod surface;
pub mod traits;
pub mod truncate;
pub mod types;
pub mod workspace;

#[cfg(test)]
mod tool_test;
#[cfg(test)]
mod types_test;

pub use adaptive::AdaptiveRegistry;
pub use full_output::{
    FULL_OUTPUT_TOOL_NAME,
    RetrieveFullOutputTool,
    full_output_tool,
};
pub use grouped::{GroupError, GroupedRegistry};
pub use output::{RenderKind, ToolOutputDefinition};
pub use registry::{
    RegistrationScope,
    RegistrationToken,
    ToolAnnotations,
    ToolCategory,
    ToolDescriptor,
    ToolEntry,
    ToolExposure,
    ToolMetadataSnapshot,
    ToolProvenance,
    ToolRegistry,
};
pub use restriction::{RestrictedRegistry, ToolRestriction};
pub use schema_builder::{ToolFeatures, ToolSchemaBuilder};
pub use structured_output::{
    STRUCTURED_OUTPUT_TOOL_NAME,
    structured_output_tool,
};
pub use surface::{
    SurfacePolicyError,
    ToolSurfacePolicy,
    called_tool_names,
    project_tool_definitions,
};
pub use traits::{ExecutionMode, StreamOutput, Tool};
pub use truncate::{
    CleanupTask,
    OutputBound,
    OverflowStrategy,
    SanitizationPolicy,
    bound_output,
    start_cleanup_task,
};
// `signal_of` reads `ExitStatus::signal()`, a POSIX-only std
// extension. Both the source (`types::signal_of`) and the
// `synthia_tool-shell` consumer gate the call site to
// `#[cfg(unix)]`; mirror that here so the re-export doesn't
// fail to resolve on windows-gnu builds. Consumers on non-unix
// targets should use `ExitStatus::code()` instead.
#[cfg(unix)]
pub use types::signal_of;
pub use types::{Context, Result, ToolOutput, TruncatedBy};
