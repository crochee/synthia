//! # synthia-tool-search
//!
//! The `search` tool — and nothing else: one [`Tool`] implementation
//! that fans a query out across every engine registered in a
//! [`Registry`], i.e. the tool / mcp / skill / memory / agent
//! catalogs the host has indexed, and returns the agent-facing
//! projection (domain, id, title, why, score) best match first.
//!
//! ## Cross-domain by construction
//!
//! A [`Registry`] is keyed by the *domain label* the host picks at
//! registration time, so the host builds one registry, registers one
//! `SearchEngine<Tool>` under `"tool"`, one `SearchEngine<Skill>`
//! under `"skill"`, and adapter engines for the async retrievers
//! (session full-text, memory recall) — this crate needs no
//! knowledge of the domains: [`Registry::search`](Registry::search)
//! merges every engine's hits, stamps each with its domain label
//! and [`synthia_search::agent_view`] renders them. Teaching the
//! tool a new domain is the host's `register` call, not a change
//! here — the same seam the `Registry` doc-comment describes.
//!
//! ## Deferred exposure
//!
//! [`register_search_tool`] registers with [`ToolExposure::Deferred`]:
//! the cold-start tool list carries name + description with a
//! deliberately permissive placeholder schema, and the real argument
//! schema (`query` required, `limit` 1..=50) is promoted once the
//! transcript mentions a `search` call — Anthropic's Tool-Search
//! pattern on this repo's existing exposure seam. The tool stays
//! callable the whole time; only its *advertisement* is staged. A
//! model that never needs cross-domain discovery never pays for the
//! schema.
//!
//! Retrieval itself is read-only and CPU-bound, so the tool keeps the
//! default [`ExecutionMode::Parallel`](synthia_tool::ExecutionMode::Parallel)
//! and may run alongside its siblings.
//!
//! ## Wiring it up
//!
//! ```
//! use std::sync::Arc;
//!
//! use synthia_core::registry::RegistryItem;
//! use synthia_search::{
//!     CjkTokenizer, Embedder, HashingEmbedder, Registry, SearchEngine,
//!     Searchable, Tokenizer,
//! };
//! use synthia_tool::{Context, Tool as _, ToolRegistry};
//!
//! #[derive(Clone)]
//! struct Skill {
//!     id: String,
//!     title: String,
//!     body: String,
//! }
//!
//! impl RegistryItem for Skill {
//!     fn name(&self) -> &str {
//!         &self.id
//!     }
//!
//!     fn description(&self) -> &str {
//!         &self.title
//!     }
//! }
//!
//! impl Searchable for Skill {
//!     fn indexed_fields(&self) -> Vec<(String, f32)> {
//!         vec![(self.title.clone(), 2.0), (self.body.clone(), 1.0)]
//!     }
//! }
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let tk: Arc<dyn Tokenizer> = Arc::new(CjkTokenizer);
//! let emb: Arc<dyn Embedder> = Arc::new(HashingEmbedder::new(64, tk.clone()));
//! let skills: SearchEngine<Skill> = SearchEngine::new(emb, tk);
//! skills
//!     .add(Skill {
//!         id: "skill:pdf".into(),
//!         title: "pdf 提取".into(),
//!         body: "把 PDF 解析成文本".into(),
//!     })
//!     .unwrap();
//!
//! let search_registry = Arc::new(Registry::new());
//! search_registry.register(skills);
//!
//! // Advertised as name + description until the first call…
//! let tools = ToolRegistry::new();
//! assert!(synthia_tool_search::register_search_tool(
//!     &tools,
//!     Arc::clone(&search_registry),
//! ));
//!
//! // …and callable immediately: the model's second call is a call.
//! let tool = synthia_tool_search::SearchTool::new(search_registry);
//! let out = tool
//!     .call(
//!         serde_json::json!({"query": "pdf 提取"}),
//!         &Context::default(),
//!     )
//!     .await;
//! assert_eq!(out.is_error, None);
//! let text: String = out
//!     .content
//!     .iter()
//!     .filter_map(|part| part.text())
//!     .collect::<Vec<_>>()
//!     .join("\n");
//! assert!(text.contains("skill:pdf"), "{text}");
//! # }
//! ```

use std::sync::Arc;

use synthia_search::Registry;
use synthia_tool::{
    Context,
    Tool,
    ToolAnnotations,
    ToolEntry,
    ToolExposure,
    ToolOutput,
};

/// The name the model calls: `search`.
pub const SEARCH_TOOL_NAME: &str = "search";

/// Hits returned when the model does not ask for a `limit`.
const DEFAULT_LIMIT: usize = 8;

/// Ceiling for `limit` — a discovery call returns candidates, not a
/// corpus dump.
const MAX_LIMIT: usize = 50;

/// The `search` tool over a host-built [`Registry`].
pub struct SearchTool {
    registry: Arc<Registry>,
}

impl SearchTool {
    /// Build the tool over `registry`.
    ///
    /// The registry is held (not copied), so the host may keep
    /// registering domains after the tool is in its registry —
    /// including lazily, behind the same `Arc`.
    pub fn new(registry: Arc<Registry>) -> Self {
        Self { registry }
    }
}

/// The model-supplied arguments of one `search` call.
#[derive(serde::Deserialize)]
struct SearchArgs {
    query: String,
    limit: Option<usize>,
    /// Restrict the call to one domain label (`"tool"`,
    /// `"skill"`, `"mcp"`, `"memory"`, `"agent"`, `"session"` —
    /// whatever the host registered). Absent searches all.
    domain: Option<String>,
}

#[async_trait::async_trait]
impl Tool for SearchTool {
    fn name(&self) -> &str {
        SEARCH_TOOL_NAME
    }

    fn description(&self) -> &str {
        "Search every catalog the host registered — tools, skills, \
         MCP tools, memory, peer agents and past sessions — by \
         keyword and meaning. Returns domain, id, title and a \
         one-line why per hit, best match first. Use it to \
         discover what is available before asking for details."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "query": {
                    "type": "string",
                    "description": "What to look for, in natural language."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_LIMIT,
                    "description": "Max hits to return (default 8)."
                },
                "domain": {
                    "type": "string",
                    "description": "Restrict to one catalog — tool, skill, \
                     mcp, memory, agent or session. Omit to search all."
                }
            },
            "required": ["query"]
        })
    }

    /// R124: MCP-native descriptor hints. `search` is read-only and
    /// returns the same hits for the same query against an
    /// unchanged registry (idempotent within a session). The
    /// `open_world_hint` is left to the host — the registry's
    /// engines decide what an `open_world` is (memory recall is
    /// local; session recall is on-disk; tool/skill catalogs are
    /// metadata). Default `false` is the right conservative
    /// setting for the model-facing tool surface.
    fn annotations(&self) -> Option<ToolAnnotations> {
        Some(ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
        })
    }

    /// Validation errors are results the model reads and can correct —
    /// an unknown argument shape or a blank query never reaches the
    /// engines.
    async fn call(
        &self,
        input: serde_json::Value,
        _context: &Context,
    ) -> ToolOutput {
        let args: SearchArgs = match serde_json::from_value(input) {
            Ok(args) => args,
            Err(e) => {
                return ToolOutput::error(format!("Invalid arguments: {e}"));
            }
        };
        if args.query.trim().is_empty() {
            return ToolOutput::error("`query` must not be empty.");
        }
        let limit = args.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let mut ctx =
            synthia_search::QueryContext::new(args.query.trim()).top_k(limit);
        if let Some(domain) = args
            .domain
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            ctx = ctx.domains([domain.to_string()]);
        }
        // The projection is `search_tool_ctx`'s verbatim output: it
        // already fans across every engine and renders the agent
        // view, so a second rendering here could only drift from it.
        ToolOutput::text(
            synthia_search::search_tool_ctx(&self.registry, ctx).await,
        )
    }
}

/// Register the `search` tool with [`ToolExposure::Deferred`].
///
/// Returns the underlying
/// [`ToolRegistry::register_entry`](synthia_tool::ToolRegistry::register_entry)
/// result: `false` only when a `Core`-provenance tool already owns the
/// name, i.e. the plugin must not shadow the harness's own tool.
pub fn register_search_tool(
    tool_registry: &synthia_tool::ToolRegistry,
    search_registry: Arc<Registry>,
) -> bool {
    tool_registry.register_entry(
        ToolEntry::new(Arc::new(SearchTool::new(search_registry)))
            .with_exposure(ToolExposure::Deferred),
    )
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, sync::Arc};

    use serde_json::json;
    use synthia_core::registry::RegistryItem;
    use synthia_search::{
        CjkTokenizer,
        Embedder,
        HashingEmbedder,
        Memory,
        Registry,
        SearchEngine,
        Searchable,
        Tokenizer,
    };
    use synthia_tool::{Context, ToolOutput, ToolRegistry};

    use super::*;

    /// A minimal `Searchable` stand-in for a skill-catalog entry.
    #[derive(Clone)]
    struct Skill {
        id: String,
        title: String,
        body: String,
    }

    impl RegistryItem for Skill {
        fn name(&self) -> &str {
            &self.id
        }

        fn description(&self) -> &str {
            &self.title
        }
    }

    impl Searchable for Skill {
        fn indexed_fields(&self) -> Vec<(String, f32)> {
            vec![(self.title.clone(), 2.0), (self.body.clone(), 1.0)]
        }
    }

    fn skill(id: &str, title: &str, body: &str) -> Skill {
        Skill {
            id: id.into(),
            title: title.into(),
            body: body.into(),
        }
    }

    fn tokenizer() -> Arc<dyn Tokenizer> {
        Arc::new(CjkTokenizer)
    }

    fn embedder(tokenizer: &Arc<dyn Tokenizer>) -> Arc<dyn Embedder> {
        Arc::new(HashingEmbedder::new(64, Arc::clone(tokenizer)))
    }

    /// One `Skill` engine holding `docs`.
    fn registry_with(docs: Vec<Skill>) -> Arc<Registry> {
        let tk = tokenizer();
        let engine: SearchEngine<Skill> = SearchEngine::new(embedder(&tk), tk);
        for doc in docs {
            engine.add(doc).unwrap();
        }
        let registry = Registry::new();
        registry.register(engine);
        Arc::new(registry)
    }

    /// Two engines under one Registry with friendly domain labels —
    /// the cross-domain case the tool exists for.
    fn two_domain_registry() -> Arc<Registry> {
        let tk = tokenizer();
        let skills: SearchEngine<Skill> =
            SearchEngine::new(embedder(&tk), Arc::clone(&tk));
        skills
            .add(skill("skill:pdf", "pdf 提取", "把 PDF 解析成文本"))
            .unwrap();
        let memories: SearchEngine<Memory> =
            SearchEngine::new(embedder(&tk), tk);
        memories
            .add(Memory {
                id: "memory:pref".into(),
                summary: "PDF 解析偏好".into(),
                content: "用户偏好把 PDF 解析为纯文本".into(),
                tags: vec![],
                created_at: 0.0,
                importance: 1.0,
            })
            .unwrap();
        let registry = Registry::new();
        registry.register_domain("skill", skills);
        registry.register_domain("memory", memories);
        Arc::new(registry)
    }

    fn ctx() -> Context {
        Context::default()
    }

    /// The textual projection of a `ToolOutput`.
    fn text_of(output: &ToolOutput) -> String {
        output
            .content
            .iter()
            .filter_map(|part| part.text().map(str::to_string))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Assert the output is a model-facing error and return its text.
    fn error_of(output: &ToolOutput) -> String {
        assert_eq!(output.is_error, Some(true), "expected an error output");
        text_of(output)
    }

    /// The agent-view hit list the model receives.
    fn hits_of(output: &ToolOutput) -> Vec<serde_json::Value> {
        serde_json::from_str(&text_of(output)).expect("agent-view JSON")
    }

    #[tokio::test]
    async fn name_and_parameters_shape() {
        let t = SearchTool::new(registry_with(vec![skill("s1", "t", "b")]));
        assert_eq!(t.name(), SEARCH_TOOL_NAME);
        assert_eq!(t.name(), "search");
        let p = t.parameters();
        assert_eq!(p["type"], "object");
        assert_eq!(
            p["additionalProperties"], false,
            "search schema must reject unknown keys (R124)"
        );
        assert_eq!(p["properties"]["query"]["type"], "string");
        assert_eq!(p["properties"]["limit"]["type"], "integer");
        assert_eq!(p["required"], json!(["query"]));
    }

    /// R124: pin the MCP-native descriptor hints — read-only and
    /// idempotent within a session; `open_world_hint` is left to
    /// the host's registry.
    #[tokio::test]
    async fn annotations_are_mcp_native_read_only() {
        let t = SearchTool::new(registry_with(vec![skill("s1", "t", "b")]));
        let a = t
            .annotations()
            .expect("search declares MCP-style annotations");
        assert_eq!(a.read_only_hint, Some(true));
        assert_eq!(a.destructive_hint, Some(false));
        assert_eq!(a.idempotent_hint, Some(true));
        assert_eq!(a.open_world_hint, Some(false));
    }

    #[tokio::test]
    async fn call_returns_agent_view_json() {
        let t = SearchTool::new(registry_with(vec![skill(
            "s1",
            "release notes",
            "wheel landed",
        )]));
        let out = t
            .call(json!({"query": "release notes", "limit": 4}), &ctx())
            .await;
        assert_eq!(out.is_error, None);
        let text = text_of(&out);
        assert!(text.contains("s1"), "hit id must appear: {text}");
    }

    #[tokio::test]
    async fn limit_defaults_to_8_and_caps_at_50() {
        let docs = (0..60)
            .map(|i| {
                skill(
                    &format!("s{i}"),
                    &format!("common doc {i}"),
                    "common token shared",
                )
            })
            .collect();
        let t = SearchTool::new(registry_with(docs));

        let default = t.call(json!({"query": "common"}), &ctx()).await;
        assert_eq!(default.is_error, None);
        assert_eq!(hits_of(&default).len(), 8, "no limit ⇒ 8");

        let capped = t
            .call(json!({"query": "common", "limit": 100}), &ctx())
            .await;
        assert_eq!(capped.is_error, None);
        assert_eq!(hits_of(&capped).len(), 50, "limit caps at 50");

        let floored =
            t.call(json!({"query": "common", "limit": 0}), &ctx()).await;
        assert_eq!(floored.is_error, None);
        assert_eq!(hits_of(&floored).len(), 1, "limit floors at 1");
    }

    #[tokio::test]
    async fn missing_query_is_model_facing_error() {
        let t = SearchTool::new(registry_with(vec![skill("s1", "t", "b")]));
        let out = t.call(json!({}), &ctx()).await;
        let text = error_of(&out);
        assert!(text.contains("Invalid arguments"), "{text}");
        assert!(text.contains("query"), "{text}");
    }

    #[tokio::test]
    async fn blank_query_is_model_facing_error() {
        let t = SearchTool::new(registry_with(vec![skill("s1", "t", "b")]));
        let out = t.call(json!({"query": "   "}), &ctx()).await;
        assert!(error_of(&out).contains("query"));
    }

    #[tokio::test]
    async fn empty_registry_is_empty_json_array_not_error() {
        let t = SearchTool::new(Arc::new(Registry::new()));
        let out = t.call(json!({"query": "x"}), &ctx()).await;
        assert_eq!(out.is_error, None);
        assert_eq!(text_of(&out), "[]");
    }

    #[tokio::test]
    async fn cross_domain_query_returns_hits_from_every_engine() {
        let t = SearchTool::new(two_domain_registry());
        let out = t.call(json!({"query": "pdf", "limit": 10}), &ctx()).await;
        assert_eq!(out.is_error, None);
        let hits = hits_of(&out);
        let ids: Vec<&str> = hits
            .iter()
            .map(|h| h["id"].as_str().unwrap_or_default())
            .collect();
        assert!(
            ids.contains(&"skill:pdf"),
            "skills engine must be searched: {ids:?}"
        );
        assert!(
            ids.contains(&"memory:pref"),
            "memory engine must be searched: {ids:?}"
        );
        for h in &hits {
            let domain = h["domain"].as_str().unwrap_or_default();
            let id = h["id"].as_str().unwrap_or_default();
            let expected = if id.starts_with("skill:") {
                "skill"
            } else {
                "memory"
            };
            assert_eq!(domain, expected, "domain label on {id}: {h:?}");
        }
    }

    #[tokio::test]
    async fn domain_param_restricts_the_fan_out() {
        let t = SearchTool::new(two_domain_registry());
        let out = t
            .call(json!({"query": "pdf", "domain": "memory"}), &ctx())
            .await;
        assert_eq!(out.is_error, None);
        let hits = hits_of(&out);
        assert!(!hits.is_empty());
        assert!(
            hits.iter().all(|h| h["domain"] == "memory"),
            "every hit must come from the memory engine: {hits:?}"
        );
    }
    #[tokio::test]
    async fn register_helper_sets_deferred_exposure() {
        let tools = ToolRegistry::new();
        let ok = register_search_tool(
            &tools,
            registry_with(vec![skill("s1", "t", "b")]),
        );
        assert!(ok);
        let d = tools
            .descriptors()
            .into_iter()
            .find(|d| d.name == SEARCH_TOOL_NAME)
            .expect("search registered");
        assert_eq!(d.exposure, synthia_tool::ToolExposure::Deferred);
    }

    #[tokio::test]
    async fn deferred_projection_hides_schema_before_first_call() {
        let tools = ToolRegistry::new();
        register_search_tool(
            &tools,
            registry_with(vec![skill("s1", "t", "b")]),
        );
        let defs = synthia_tool::project_tool_definitions(
            &tools.descriptors(),
            &Default::default(),
            None,
        );
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].input_schema["additionalProperties"], true);
    }

    #[tokio::test]
    async fn deferred_schema_promotes_to_the_tools_own_parameters() {
        let tools = ToolRegistry::new();
        register_search_tool(
            &tools,
            registry_with(vec![skill("s1", "t", "b")]),
        );
        let called: HashSet<String> =
            HashSet::from([SEARCH_TOOL_NAME.to_string()]);
        let defs = synthia_tool::project_tool_definitions(
            &tools.descriptors(),
            &called,
            None,
        );
        assert_eq!(defs.len(), 1);
        let t = SearchTool::new(registry_with(vec![skill("s1", "t", "b")]));
        assert_eq!(defs[0].input_schema, t.parameters());
    }

    #[test]
    fn execution_mode_is_parallel() {
        let t = SearchTool::new(registry_with(vec![skill("s1", "t", "b")]));
        assert!(matches!(t.mode(), synthia_tool::ExecutionMode::Parallel));
    }

    #[test]
    fn description_mentions_cross_domain() {
        let t = SearchTool::new(registry_with(vec![skill("s1", "t", "b")]));
        let d = t.description();
        assert!(
            d.contains("skill") && d.contains("memory"),
            "description must name the domains it spans: {d}"
        );
    }
}
