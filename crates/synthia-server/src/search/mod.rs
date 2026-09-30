//! Cross-domain search: the one [`Registry`] behind both
//! capabilities.
//!
//! 1. **Agent lazy discovery** — the registry is registered as the
//!    model-facing `search` tool
//!    ([`synthia::tool_search::register_search_tool`], deferred
//!    exposure), so an agent discovers tools / skills / MCP tools /
//!    memory / peer agents / past sessions by querying instead of
//!    the host enumerating everything into its context.
//! 2. **Frontend search** — `GET /api/v1/search`
//!    ([`crate::routes::search`]) projects the same registry, so
//!    the UI's search box and the model's `search` call are one
//!    engine, not two code paths that drift.
//!
//! # Where the wrap lives
//!
//! Per the layering rule, the whole projection lives in
//! `synthia-server`: every domain row becomes a `SearchDoc`, and the
//! two corpus domains delegate to their own retrievers
//! (`MemoryEngine` over `Memory::recall`, `SessionEngine` over
//! `SessionSearch`). No `synthia-*`
//! library crate — and nothing in the agent runtime — learns about
//! search or users.
//!
//! # Freshness
//!
//! Engines own their own staleness: the catalog domains re-probe
//! their source version on every query and rebuild only on change
//! (skills refresh on a clock bucket, since skill files carry no
//! version counter), and the two retriever domains answer live by
//! construction.

mod adapters;
mod catalog;
mod doc;

use std::sync::Arc;

use adapters::{MemoryEngine, SessionEngine};
use catalog::CatalogEngine;
use doc::SearchDoc;
use synthia::{
    context::Memory,
    core::Clock,
    harness::AgentRegistry,
    search::{Hit, QueryContext, Registry},
    session::SharedSessionSearch,
    tool::{ToolExposure, ToolRegistry},
};
use tokio::sync::RwLock;

/// Domain label: server tool plugins (`read` / `write` / `shell` …).
pub const DOMAIN_TOOL: &str = "tool";
/// Domain label: tools published by configured MCP servers
/// (`mcp__<server>__<tool>`).
pub const DOMAIN_MCP: &str = "mcp";
/// Domain label: discovered `SKILL.md` files.
pub const DOMAIN_SKILL: &str = "skill";
/// Domain label: long-term memory entries.
pub const DOMAIN_MEMORY: &str = "memory";
/// Domain label: registered peer agents.
pub const DOMAIN_AGENT: &str = "agent";
/// Domain label: past session transcripts (full-text).
pub const DOMAIN_SESSION: &str = "session";

/// [`QueryContext::extra`] key carrying the requesting user id —
/// a server-layer routing fact the session engine scopes by. The
/// agent-side `search` tool never sets it (single-tenant store);
/// the HTTP handler always does.
pub const EXTRA_USER_SCOPE: &str = "user_id";

/// How often the skill catalog re-reads the workspace, in seconds.
/// Skill files carry no version counter; a one-minute bucket caps
/// the walk at one `discover_skills` per minute under load.
const SKILL_REFRESH_SECS: i64 = 60;

/// Every live source the six engines read.
pub struct SearchSources {
    pub tool_registry: Arc<RwLock<ToolRegistry>>,
    pub agent_registry: Arc<AgentRegistry>,
    pub workspace_root: std::path::PathBuf,
    pub memory: Arc<dyn Memory>,
    pub session_search: SharedSessionSearch,
    pub clock: synthia::core::SharedClock,
}

/// The cross-domain search service. Two views over one set of
/// engines:
///
/// - `http` — all six domains. The session domain answers with the
///   **requesting user's** transcripts because the HTTP handler
///   always sets [`EXTRA_USER_SCOPE`].
/// - `agent` — the five catalog domains. Session transcripts are
///   user data with no request to scope them to on the agent path
///   (the model's `search` call carries no user, by design — user
///   identity is host-layer business), so the deployment-wide
///   transcript index is not advertised to the model at all.
///   Per-user session search stays where the user is known: the
///   HTTP surface (`GET /api/v1/search`,
///   `GET /api/v1/sessions/search`).
pub struct SearchService {
    http: Arc<Registry>,
    agent: Arc<Registry>,
}

/// Project the registry's tool descriptors into search docs,
/// split by the `mcp__` namespace.
fn tool_docs(registry: &ToolRegistry, mcp_only: bool) -> Vec<SearchDoc> {
    registry
        .descriptors_cached()
        .iter()
        .filter(|d| !d.is_hidden && d.exposure != ToolExposure::Hidden)
        .filter(|d| d.name.starts_with("mcp__") == mcp_only)
        .map(SearchDoc::from_tool_descriptor)
        .collect()
}

/// The two tool-registry domains share one snapshot walk: version
/// probe + filtered projection, parameterised by the `mcp__` split.
fn tool_catalog(
    tool_registry: &Arc<RwLock<ToolRegistry>>,
    mcp_only: bool,
) -> CatalogEngine {
    let probe = Arc::clone(tool_registry);
    let snap = Arc::clone(tool_registry);
    CatalogEngine::new(
        Box::new(move || probe.try_read().map(|g| g.version()).unwrap_or(0)),
        Box::new(move || {
            match snap.try_read() {
                Ok(g) => tool_docs(&g, mcp_only),
                // A writer holds the registry; answer from the
                // cached snapshot by reporting "no change".
                Err(_) => Vec::new(),
            }
        }),
    )
}

impl SearchService {
    /// Wire the engines into both views. Every engine is built
    /// once behind an `Arc`; the two registries only differ in
    /// which of them the session domain is published to.
    pub fn build(src: SearchSources) -> Arc<Self> {
        let http = Arc::new(Registry::new());
        let agent = Arc::new(Registry::new());

        let tool = Arc::new(tool_catalog(&src.tool_registry, false))
            as Arc<dyn synthia::search::ErasedEngine>;
        let mcp = Arc::new(tool_catalog(&src.tool_registry, true))
            as Arc<dyn synthia::search::ErasedEngine>;

        // Skills: no version counter, so the probe is a clock
        // bucket — at most one directory walk per minute.
        let skills_root = src.workspace_root.clone();
        let skills_clock = src.clock.clone();
        let skill = Arc::new(CatalogEngine::new(
            Box::new(move || {
                (skills_clock.now().timestamp() / SKILL_REFRESH_SECS).max(0)
                    as u64
            }),
            Box::new(move || {
                synthia::skill::discover_skills(&skills_root)
                    .iter()
                    .map(SearchDoc::from_skill)
                    .collect()
            }),
        )) as Arc<dyn synthia::search::ErasedEngine>;

        // Agents: the registry's own version counter keys the
        // snapshot; descriptors come out of the cached entries.
        let agents = Arc::clone(&src.agent_registry);
        let agents_probe = Arc::clone(&src.agent_registry);
        let agent_domain = Arc::new(CatalogEngine::new(
            Box::new(move || agents_probe.version()),
            Box::new(move || {
                agents
                    .names()
                    .iter()
                    .filter_map(|name| agents.resolve_sync(name))
                    .map(|entry| {
                        SearchDoc::from_agent_descriptor(entry.descriptor())
                    })
                    .collect()
            }),
        )) as Arc<dyn synthia::search::ErasedEngine>;

        let memory = Arc::new(MemoryEngine::new(Arc::clone(&src.memory)))
            as Arc<dyn synthia::search::ErasedEngine>;
        let session =
            Arc::new(SessionEngine::new(Arc::clone(&src.session_search)))
                as Arc<dyn synthia::search::ErasedEngine>;

        for (domain, engine) in [
            (DOMAIN_TOOL, &tool),
            (DOMAIN_MCP, &mcp),
            (DOMAIN_SKILL, &skill),
            (DOMAIN_AGENT, &agent_domain),
            (DOMAIN_MEMORY, &memory),
            (DOMAIN_SESSION, &session),
        ] {
            http.register_erased(domain, Arc::clone(engine));
        }
        // Everything but the session corpus: see the type docs for
        // why the deployment-wide transcript index stays off the
        // agent surface.
        for (domain, engine) in [
            (DOMAIN_TOOL, &tool),
            (DOMAIN_MCP, &mcp),
            (DOMAIN_SKILL, &skill),
            (DOMAIN_AGENT, &agent_domain),
            (DOMAIN_MEMORY, &memory),
        ] {
            agent.register_erased(domain, Arc::clone(engine));
        }

        Arc::new(Self { http, agent })
    }

    /// The registry the agent-facing `search` tool searches: every
    /// catalog domain, no session transcripts.
    pub fn registry(&self) -> Arc<Registry> {
        Arc::clone(&self.agent)
    }

    /// The domain labels the agent-facing tool can reach (sorted).
    pub fn domains(&self) -> Vec<String> {
        self.agent.domains()
    }

    /// The domain labels the HTTP projection serves (sorted) —
    /// [`domains`](Self::domains) plus the session corpus.
    pub fn http_domains(&self) -> Vec<String> {
        self.http.domains()
    }

    /// One cross-domain query against the HTTP view. `user_id`
    /// scopes the session domain (server-layer tenancy); empty /
    /// `None` searches the whole store, which only an operator
    /// surface should do.
    pub async fn search(
        &self,
        query: &str,
        limit: usize,
        domains: Option<Vec<String>>,
        user_id: Option<&str>,
    ) -> Vec<Hit> {
        let mut ctx = QueryContext::new(query).top_k(limit);
        if let Some(domains) = domains.filter(|d: &Vec<String>| !d.is_empty()) {
            ctx = ctx.domains(domains);
        }
        if let Some(user) = user_id.filter(|u| !u.is_empty()) {
            ctx = ctx.extra(
                EXTRA_USER_SCOPE,
                serde_json::Value::String(user.to_string()),
            );
        }
        self.http.search(&ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn agent_registry_omits_sessions_http_serves_all_six() {
        let state = crate::state::AppState::for_test(
            synthia::session::manager::SessionRegistry::new(
                std::env::temp_dir(),
            ),
            std::env::temp_dir(),
        )
        .await;

        // The agent-facing tool reaches every *catalog* domain and
        // deliberately not the session corpus (user data with no
        // request to scope it to).
        assert_eq!(
            state.search.domains(),
            vec!["agent", "mcp", "memory", "skill", "tool"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        // The HTTP projection serves all six, session included.
        assert_eq!(
            state.search.http_domains(),
            vec!["agent", "mcp", "memory", "session", "skill", "tool"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );

        // Capability 1: the agent registry is the model-facing
        // `search` tool, deferred so the cold tool list stays
        // name + description.
        let g = state.tool_registry.read().await;
        let d = g
            .descriptors()
            .into_iter()
            .find(|d| d.name == "search")
            .expect("search tool registered");
        assert_eq!(d.exposure, ToolExposure::Deferred);
    }

    #[tokio::test]
    async fn cross_domain_query_hits_tools_and_memory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = crate::state::AppState::for_test(
            synthia::session::manager::SessionRegistry::new(
                temp.path().to_path_buf(),
            ),
            temp.path().to_path_buf(),
        )
        .await;
        state
            .memory
            .store(synthia::context::MemoryEntry::now(
                "m-pdf",
                "用户偏好把 pdf 解析为纯文本",
            ))
            .await
            .expect("seed memory");

        // Cross-domain: the memory entry and (for a file-flavoured
        // query) tool docs answer the same query.
        let hits = state.search.search("pdf", 10, None, None).await;
        let memory_hit = hits
            .iter()
            .find(|h| h.domain == "memory")
            .expect("memory domain must answer a cross-domain query");
        assert_eq!(memory_hit.id, "m-pdf");
        assert!(memory_hit.preview.is_some());

        let tool_hits = state.search.search("file", 10, None, None).await;
        assert!(
            tool_hits.iter().any(|h| h.domain == "tool"),
            "plugin tools must be searchable: {:?}",
            tool_hits.iter().map(|h| h.id.clone()).collect::<Vec<_>>()
        );

        // Domain filter: tools only, memory must not answer.
        let tools_only = state
            .search
            .search("pdf", 10, Some(vec!["tool".into()]), None)
            .await;
        assert!(tools_only.iter().all(|h| h.domain == "tool"));
    }
}
