//! Multi-agent registry. See [`AgentRegistry`].

use std::collections::HashMap;

use async_trait::async_trait;
use parking_lot::RwLock;
use synthia_core::{
    error::Error,
    registry::{Registry, RegistryItem, paginate_registry_list},
};

use super::{
    Agent,
    descriptor::{AgentEntry, AgentFilter},
};

/// Multi-agent registry. Implements
/// [`synthia_core::registry::Registry`] with
/// `Item = AgentEntry`; `list_paginate` delegates the
/// slicing/cursor/envelope work to
/// [`synthia_core::registry::paginate_registry_list`] after
/// applying the filter + sort.
pub struct AgentRegistry {
    inner: RwLock<HashMap<String, AgentEntry>>,
    /// Monotonic version counter, bumped on every successful
    /// `register` / `unregister`. Lets callers cheaply key a
    /// snapshot cache (e.g. `list_agents` in
    /// `crates/synthia-server/src/routes/agents.rs`) by the
    /// current version without holding the registry lock or
    /// diffing the full entry list.
    version: std::sync::atomic::AtomicU64,
}

impl AgentRegistry {
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            version: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Current monotonic version of the registry. Bumped on
    /// every successful `register` / `unregister`. Cheap (one
    /// relaxed atomic load) — callers can use this to key a
    /// snapshot cache without holding the registry lock.
    pub fn version(&self) -> u64 {
        self.version.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Synchronous resolve used by callers that cannot await
    /// (e.g. the run factory when it must construct the agent
    /// synchronously before returning the event stream).
    /// Backed by [`parking_lot::RwLock`], so it does **not**
    /// block on any external I/O.
    pub fn resolve_sync(
        &self,
        name: &str,
    ) -> Option<std::sync::Arc<dyn Agent>> {
        let guard = self.inner.read();
        guard.get(name).map(AgentEntry::agent)
    }

    /// Synchronous snapshot of all registered agent names.
    pub fn names(&self) -> Vec<String> {
        let guard = self.inner.read();
        guard.keys().cloned().collect()
    }

    /// Return the first registered agent name, without
    /// materialising the full name list. The previous
    /// implementation called `names().into_iter().next()`,
    /// which allocated a `Vec<String>` of every registered
    /// name just to discard all but the first — wasteful for
    /// the dispatch hot path (every chat reply calls
    /// `resolve_agent_name` to figure out which descriptor
    /// to load).
    pub fn first_name(&self) -> Option<String> {
        let guard = self.inner.read();
        guard.keys().next().cloned()
    }

    /// Number of registered agents.
    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    /// Whether the registry has no registered agents.
    pub fn is_empty(&self) -> bool {
        self.inner.read().is_empty()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Registry for AgentRegistry {
    type Filter = AgentFilter;
    type Item = AgentEntry;

    async fn put(&self, item: Self::Item) -> Result<(), Error> {
        let name = item.name().to_string();
        let mut guard = self.inner.write();
        if guard.contains_key(&name) {
            return Err(Error::already_exists(name));
        }
        guard.insert(name, item);
        self.version
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn delete(&self, name: &str) -> Result<(), Error> {
        let mut guard = self.inner.write();
        if guard.remove(name).is_none() {
            return Err(Error::not_found(name));
        }
        self.version
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn get(&self, name: &str) -> Result<Option<Self::Item>, Error> {
        Ok(self.inner.read().get(name).cloned())
    }

    async fn list_paginate(
        &self,
        cursor: Option<String>,
        limit: u64,
        _sort: Option<String>,
        filter: Option<Self::Filter>,
    ) -> Result<synthia_core::registry::RegistryList<Self::Item>, Error> {
        let filter = filter.unwrap_or_default();
        let guard = self.inner.read();
        let mut out: Vec<AgentEntry> = guard
            .values()
            .filter(|e| {
                let d = e.descriptor();
                filter.kind.as_deref().is_none_or(|k| d.kind == k)
                    && filter
                        .capability
                        .as_deref()
                        .is_none_or(|c| d.capabilities.iter().any(|t| t == c))
                    && filter
                        .tool
                        .as_deref()
                        .is_none_or(|t| d.tools.iter().any(|n| n == t))
                    && filter
                        .min_version
                        .as_deref()
                        .is_none_or(|v| d.version.as_str() >= v)
                    && filter
                        .handoff
                        .as_deref()
                        .is_none_or(|h| d.handoffs.iter().any(|n| n == h))
                    && filter
                        .owner
                        .as_deref()
                        .is_none_or(|o| d.owner.as_deref() == Some(o))
                    && filter
                        .domain
                        .as_deref()
                        .is_none_or(|o| d.domain.as_deref() == Some(o))
            })
            .cloned()
            .collect();
        out.sort_by(|a, b| a.name().cmp(b.name()));
        // Sort is applied above; cursor + limit + envelope come
        // from the shared pagination primitive.
        paginate_registry_list(out, cursor.as_deref(), limit)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use synthia_core::{
        error::Error,
        registry::{Registry, RegistryItem},
    };

    use super::*;
    use crate::agent::{
        Agent,
        descriptor::{AgentDescriptor, AgentEntry},
    };

    /// Stub agent used for registry tests.
    struct StubAgent {
        desc: AgentDescriptor,
    }

    #[async_trait::async_trait]
    impl Agent for StubAgent {
        fn descriptor(&self) -> &AgentDescriptor {
            &self.desc
        }

        async fn run(
            &self,
            _input: crate::input::AgentInput,
            _cancel: std::sync::Arc<tokio_util::sync::CancellationToken>,
        ) -> std::pin::Pin<
            Box<
                dyn futures::Stream<Item = crate::events::AgentEvent>
                    + Send
                    + 'static,
            >,
        > {
            Box::pin(futures::stream::empty())
        }
    }

    impl RegistryItem for StubAgent {
        fn name(&self) -> &str {
            &self.desc.name
        }

        fn description(&self) -> &str {
            &self.desc.description
        }
    }

    fn stub(name: &str, kind: &str, caps: &[&str]) -> AgentEntry {
        AgentEntry::new(Arc::new(StubAgent {
            desc: AgentDescriptor {
                name: name.into(),
                description: format!("desc for {name}"),
                kind: kind.into(),
                version: "1.0.0".into(),
                instructions: String::new(),
                capabilities: caps.iter().map(|s| s.to_string()).collect(),
                tools: Vec::new(),
                model_hint: None,
                handoffs: Vec::new(),
                handoff_hint: None,
                output_schema: None,
                owner: None,
                domain: None,
                persona: None,
                display_name: None,
            },
        }))
    }

    #[tokio::test]
    async fn put_get_delete() {
        let reg = AgentRegistry::new();
        let e = stub("a", "react", &["tools"]);
        reg.put(e).await.unwrap();

        let got = reg.get("a").await.unwrap().unwrap();
        assert_eq!(got.name(), "a");

        reg.delete("a").await.unwrap();
        assert!(reg.get("a").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn duplicate_put_returns_error() {
        let reg = AgentRegistry::new();
        reg.put(stub("a", "react", &[])).await.unwrap();
        let err = reg.put(stub("a", "react", &[])).await.unwrap_err();
        assert!(matches!(err, Error::AlreadyExists { .. }), "got {err:?}");
    }

    #[tokio::test]
    async fn unknown_delete_returns_not_found() {
        let reg = AgentRegistry::new();
        let err = reg.delete("nope").await.unwrap_err();
        assert!(matches!(err, Error::NotFound { .. }), "got {err:?}");
    }

    #[tokio::test]
    async fn list_sorted_by_name() {
        let reg = AgentRegistry::new();
        reg.put(stub("c", "react", &[])).await.unwrap();
        reg.put(stub("a", "react", &[])).await.unwrap();
        reg.put(stub("b", "react", &[])).await.unwrap();
        let items = reg.list(None).await.unwrap();
        assert_eq!(
            items.iter().map(|i| i.name()).collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
    }

    #[tokio::test]
    async fn list_filter_kind() {
        let reg = AgentRegistry::new();
        reg.put(stub("a", "react", &[])).await.unwrap();
        reg.put(stub("b", "pipeline", &[])).await.unwrap();
        let only_react = reg
            .list(Some(AgentFilter {
                kind: Some("react".into()),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(only_react.len(), 1);
        assert_eq!(only_react[0].name(), "a");
    }

    #[tokio::test]
    async fn list_filter_capability() {
        let reg = AgentRegistry::new();
        reg.put(stub("a", "react", &["tools"])).await.unwrap();
        reg.put(stub("b", "react", &["streaming"])).await.unwrap();
        let only_tools = reg
            .list(Some(AgentFilter {
                capability: Some("tools".into()),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(only_tools.len(), 1);
        assert_eq!(only_tools[0].name(), "a");
    }

    #[tokio::test]
    async fn list_filter_min_version() {
        let reg = AgentRegistry::new();
        let mut v_low = stub("a", "react", &[]);
        v_low.descriptor_mut().version = "0.9.0".into();
        let mut v_high = stub("b", "react", &[]);
        v_high.descriptor_mut().version = "1.0.0".into();
        reg.put(v_low).await.unwrap();
        reg.put(v_high).await.unwrap();
        let only_recent = reg
            .list(Some(AgentFilter {
                min_version: Some("1.0.0".into()),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(only_recent.len(), 1);
        assert_eq!(only_recent[0].name(), "b");
    }

    #[tokio::test]
    async fn list_combined_filter_is_conjunctive() {
        let reg = AgentRegistry::new();
        reg.put(stub("a", "react", &["tools"])).await.unwrap();
        reg.put(stub("b", "pipeline", &["tools"])).await.unwrap();
        let only = reg
            .list(Some(AgentFilter {
                kind: Some("react".into()),
                capability: Some("tools".into()),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].name(), "a");
    }

    #[tokio::test]
    async fn resolve_sync_returns_arc_dyn_agent() {
        let reg = AgentRegistry::new();
        reg.put(stub("a", "react", &[])).await.unwrap();
        let arc = reg.resolve_sync("a").unwrap();
        assert_eq!(arc.descriptor().name, "a");
        assert!(reg.resolve_sync("nope").is_none());
    }

    #[tokio::test]
    async fn list_paginate_reuses_core_default() {
        let reg = AgentRegistry::new();
        for n in ["a", "b", "c"] {
            reg.put(stub(n, "react", &[])).await.unwrap();
        }
        let page = reg.list_paginate(None, 2, None, None).await.unwrap();
        assert_eq!(page.data.len(), 2);
        assert_eq!(page.data[0].name(), "a");
        assert!(page.next_cursor.is_some());
        assert_eq!(page.total, Some(3));
    }

    #[tokio::test]
    async fn list_filter_tool() {
        let reg = AgentRegistry::new();
        let mut a = stub("a", "react", &[]);
        a.descriptor_mut().tools = vec!["read_file".into()];
        let mut b = stub("b", "react", &[]);
        b.descriptor_mut().tools = vec!["shell".into()];
        reg.put(a).await.unwrap();
        reg.put(b).await.unwrap();
        let only = reg
            .list(Some(AgentFilter {
                tool: Some("shell".into()),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].name(), "b");
    }

    #[tokio::test]
    async fn list_filter_handoff() {
        let reg = AgentRegistry::new();
        let mut a = stub("a", "react", &[]);
        a.descriptor_mut().handoffs = vec!["planner".into()];
        let b = stub("b", "react", &[]);
        reg.put(a).await.unwrap();
        reg.put(b).await.unwrap();
        let only = reg
            .list(Some(AgentFilter {
                handoff: Some("planner".into()),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].name(), "a");
    }

    #[tokio::test]
    async fn list_filter_owner() {
        let reg = AgentRegistry::new();
        let mut a = stub("a", "react", &[]);
        a.descriptor_mut().owner = Some("team-a".into());
        let mut b = stub("b", "react", &[]);
        b.descriptor_mut().owner = Some("team-b".into());
        reg.put(a).await.unwrap();
        reg.put(b).await.unwrap();
        let only = reg
            .list(Some(AgentFilter {
                owner: Some("team-b".into()),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].name(), "b");
    }

    #[tokio::test]
    async fn list_filter_domain() {
        let reg = AgentRegistry::new();
        let mut a = stub("a", "react", &[]);
        a.descriptor_mut().domain = Some("coding".into());
        let mut b = stub("b", "react", &[]);
        b.descriptor_mut().domain = Some("research".into());
        reg.put(a).await.unwrap();
        reg.put(b).await.unwrap();
        let only = reg
            .list(Some(AgentFilter {
                domain: Some("research".into()),
                ..Default::default()
            }))
            .await
            .unwrap();
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].name(), "b");
    }
}
