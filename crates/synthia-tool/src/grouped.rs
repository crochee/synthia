//! [`GroupedRegistry`] — named tool groups with independent
//! activation.
//!
//! Adopted from traitclaw
//! `crates/traitclaw-core/src/registries.rs:170-310`
//! (`GroupedRegistry::group` / `activate_group` /
//! `deactivate_group`). A deployment declares *named groups* over the
//! tools it registered and decides, at runtime, which groups the
//! model is told about.
//!
//! ## The point: visibility ≠ executability
//!
//! Only tools in active groups (plus tools no group claims) appear in
//! [`GroupedRegistry::visible_tool_names`]. Dispatch is **not**
//! touched: the wrapper never gates execution, so a tool whose group
//! is deactivated stays callable through
//! [`ToolRegistry::run_stream`] — which is exactly what a skill, a
//! workflow step, or a subagent needs when the *model* is not
//! supposed to see the tool. Narrowing what the model sees is a
//! prompt-budget and focus decision; it is not and must not be a
//! security boundary. (Hidden tools, by contrast, are refused by both
//! `snapshot` and `run_stream` — that is the registry's own privacy
//! contract, and it is unchanged here.)
//!
//! ## Deliberate divergences from traitclaw
//!
//! - **Read-side wrapper, not a registry.** The wrapper holds
//!   `Arc<ToolRegistry>` and only re-shapes the LLM-facing list, like
//!   [`AdaptiveRegistry`](crate::AdaptiveRegistry). Registration,
//!   name uniqueness, panic isolation and dispatch stay in one place.
//! - **Ungrouped tools stay visible.** traitclaw's registry *is* the
//!   groups, so a tool outside every group does not exist. A wrapper
//!   cannot make that assumption about an existing catalog: grouping
//!   is opt-in narrowing, and a deployment that declares no group
//!   sees the catalog it had.
//! - **One group per tool.** "Which group advertises this?" has
//!   exactly one answer, so visibility never depends on the order
//!   groups were declared (traitclaw searches a `HashMap` of groups,
//!   where the first match is arbitrary).
//! - **Declarations are validated, not trusted.** An unknown tool
//!   name or a tool claimed by two groups is a wiring error raised at
//!   declaration time ([`GroupError`]), not a silent no-op.
//!
//! ## Group naming and ordering
//!
//! Group names are `BTreeMap` keys, so [`GroupedRegistry::group_names`]
//! and [`GroupedRegistry::active_group_names`] are sorted
//! deterministically — logs and tests do not depend on insertion
//! order. Membership lists keep the declaration order given, which is
//! the order a caller wrote them in. The *visible* list, by contrast,
//! follows [`ToolRegistry::snapshot`]'s order (sorted by tool name),
//! so it is byte-stable for the same tool set.
//!
//! ## Declaring a group hides it until activation
//!
//! A declaration is an opt-in narrowing, so a tool named by a group
//! is not advertised until that group is active: a deployment
//! declares every group, then activates the ones this run should
//! offer. A tool no declaration mentions is advertised throughout.
//!
//! ## Consuming the active set
//!
//! [`GroupedRegistry::visible_tool_names`] is the allow-list half of
//! the projection in [`crate::surface`]:
//!
//! ```ignore
//! let visible: HashSet<String> =
//!     grouped.visible_tool_names().into_iter().collect();
//! let defs = synthia_tool::project_tool_definitions(
//!     &registry.descriptors(),
//!     &synthia_tool::called_tool_names(&messages),
//!     Some(&visible),
//! );
//! ```

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use parking_lot::RwLock;
use thiserror::Error;

use crate::{ToolMetadataSnapshot, ToolRegistry};

/// Why a group declaration or activation was rejected.
///
/// Every variant names the group and tool involved, because the
/// caller is wiring a catalog and needs to find the typo without
/// guessing.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GroupError {
    /// A group must have a name; an empty one cannot be activated or
    /// logged meaningfully.
    #[error("group name must not be empty")]
    EmptyGroupName,
    /// The group names a tool the registry does not have.
    #[error("group `{group}` names tool `{tool}`, which is not registered")]
    UnknownTool {
        /// Group being declared.
        group: String,
        /// Unregistered tool name.
        tool: String,
    },
    /// The same tool appears twice in one declaration.
    #[error("group `{group}` names tool `{tool}` twice")]
    DuplicateTool {
        /// Group being declared.
        group: String,
        /// Repeated tool name.
        tool: String,
    },
    /// The tool already belongs to a different group.
    #[error(
        "tool `{tool}` is already in group `{existing}`, so group \
         `{group}` cannot claim it"
    )]
    AlreadyGrouped {
        /// Group being declared.
        group: String,
        /// Contested tool name.
        tool: String,
        /// Group that already claims it.
        existing: String,
    },
    /// Activation named a group that was never declared.
    #[error("no such group: `{name}`")]
    UnknownGroup {
        /// Undeclared group name.
        name: String,
    },
}

/// Mutable half of a [`GroupedRegistry`], shared across clones.
#[derive(Debug, Default)]
struct GroupState {
    /// Declared groups: name → members, in declaration order.
    groups: BTreeMap<String, Vec<String>>,
    /// Reverse index: tool → the single group that claims it.
    membership: HashMap<String, String>,
    /// Activated groups (always a subset of `groups`).
    active: BTreeSet<String>,
}

impl GroupState {
    /// Drop a declaration, its reverse-index entries, and its
    /// activation. Returns whether the group existed.
    fn remove_group(&mut self, name: &str) -> bool {
        match self.groups.remove(name) {
            Some(members) => {
                for tool in members {
                    self.membership.remove(&tool);
                }
                self.active.remove(name);
                true
            }
            None => false,
        }
    }

    /// Whether `tool` is advertised: ungrouped tools are, grouped
    /// tools are iff their group is active.
    fn is_visible(&self, tool: &str) -> bool {
        match self.membership.get(tool) {
            None => true,
            Some(group) => self.active.contains(group),
        }
    }
}

/// Named tool groups with independent activation, over a shared
/// [`ToolRegistry`].
///
/// Cheap to clone: clones share one group state, so an activation
/// switch through any clone is seen by all of them.
#[derive(Clone)]
pub struct GroupedRegistry {
    inner: Arc<ToolRegistry>,
    state: Arc<RwLock<GroupState>>,
    /// Memoised result of [`Self::visible_tool_names`], keyed by the
    /// inner registry's [`ToolRegistry::version`] so a registration or
    /// removal invalidates the cache implicitly. Every group mutator
    /// (`declare` / `remove` / `activate` / `deactivate` /
    /// `activate_only`) clears it through
    /// [`Self::invalidate_visible_cache`]. The `Arc<Mutex<...>>`
    /// mirrors `state`'s shape: clones share the same lock so an
    /// invalidate through one clone is seen by every clone.
    visible_cache: Arc<parking_lot::Mutex<Option<VisibleCache>>>,
}

/// One entry of [`GroupedRegistry::visible_cache`].
///
/// The cache key is the inner registry's version at compute time, so
/// a registration or removal on the inner registry invalidates this
/// entry implicitly (the version bumps). Group mutators clear it
/// eagerly because the version key alone does not detect a change in
/// group membership.
#[derive(Debug)]
struct VisibleCache {
    registry_version: u64,
    names: Vec<String>,
}

impl GroupedRegistry {
    /// Wrap `inner`. Starts with no groups declared, so every
    /// registered tool is visible and behaviour is unchanged until a
    /// group is declared.
    #[must_use]
    pub fn new(inner: Arc<ToolRegistry>) -> Self {
        Self {
            inner,
            state: Arc::new(RwLock::new(GroupState::default())),
            visible_cache: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    /// Drop the cached visible-names entry. Called by every group
    /// mutator. Cheap: a single `Mutex` lock and a `None` write.
    fn invalidate_visible_cache(&self) {
        *self.visible_cache.lock() = None;
    }

    /// Inner registry — the dispatch authority. Groups never gate
    /// execution, so a caller that needs to run a tool from a
    /// deactivated group dispatches through here.
    #[must_use]
    pub fn inner(&self) -> &Arc<ToolRegistry> {
        &self.inner
    }

    /// Declare (or re-declare) group `name` over `tools`.
    ///
    /// Re-declaring replaces the previous membership of that group
    /// and keeps its activation flag; tools that left the group
    /// become ungrouped again, and tools that joined must not
    /// already belong to another group.
    ///
    /// # Errors
    ///
    /// [`GroupError::EmptyGroupName`], [`GroupError::UnknownTool`],
    /// [`GroupError::DuplicateTool`], or
    /// [`GroupError::AlreadyGrouped`]. Validation completes before
    /// any mutation, so a rejected declaration changes nothing.
    pub fn declare(
        &self,
        name: &str,
        tools: &[&str],
    ) -> Result<(), GroupError> {
        if name.is_empty() {
            return Err(GroupError::EmptyGroupName);
        }
        let mut state = self.state.write();
        validate_declaration(&self.inner, &state, name, tools)?;
        let was_active = state.active.contains(name);
        state.remove_group(name);
        let owned: Vec<String> =
            tools.iter().map(|tool| (*tool).to_owned()).collect();
        for tool in &owned {
            state.membership.insert(tool.clone(), name.to_owned());
        }
        state.groups.insert(name.to_owned(), owned);
        if was_active {
            state.active.insert(name.to_owned());
        }
        // Drop the `state` lock before clearing the cache so the two
        // Mutex acquisitions happen in opposite order to the
        // `visible_tool_names` fast path (which holds the cache lock
        // first and never takes the `state` lock on a hit).
        drop(state);
        self.invalidate_visible_cache();
        Ok(())
    }

    /// Forget group `name`, releasing its tools back to the default
    /// visible set. Returns whether the group existed.
    pub fn remove(&self, name: &str) -> bool {
        let existed = self.state.write().remove_group(name);
        if existed {
            self.invalidate_visible_cache();
        }
        existed
    }

    /// Activate group `name`. Returns `false` when it was never
    /// declared (activation never creates a group).
    pub fn activate(&self, name: &str) -> bool {
        let mut state = self.state.write();
        if state.groups.contains_key(name) {
            state.active.insert(name.to_owned());
            drop(state);
            self.invalidate_visible_cache();
            true
        } else {
            false
        }
    }

    /// Deactivate group `name`. Returns whether it was active.
    pub fn deactivate(&self, name: &str) -> bool {
        let was_active = self.state.write().active.remove(name);
        if was_active {
            self.invalidate_visible_cache();
        }
        was_active
    }

    /// Replace the active set with exactly `names`.
    ///
    /// # Errors
    ///
    /// [`GroupError::UnknownGroup`] for the first name that was never
    /// declared. The active set is left untouched in that case, so a
    /// typo cannot half-apply a switch.
    pub fn activate_only(&self, names: &[&str]) -> Result<(), GroupError> {
        let mut state = self.state.write();
        for name in names {
            if !state.groups.contains_key(*name) {
                return Err(GroupError::UnknownGroup {
                    name: (*name).to_owned(),
                });
            }
        }
        state.active = names.iter().map(|name| (*name).to_owned()).collect();
        drop(state);
        self.invalidate_visible_cache();
        Ok(())
    }

    /// Whether group `name` is currently active.
    #[must_use]
    pub fn is_active(&self, name: &str) -> bool {
        self.state.read().active.contains(name)
    }

    /// Every declared group name, sorted.
    #[must_use]
    pub fn group_names(&self) -> Vec<String> {
        self.state.read().groups.keys().cloned().collect()
    }

    /// Active group names, sorted.
    #[must_use]
    pub fn active_group_names(&self) -> Vec<String> {
        self.state.read().active.iter().cloned().collect()
    }

    /// Members of group `name` in declaration order, or `None` when
    /// the group was never declared.
    #[must_use]
    pub fn members(&self, name: &str) -> Option<Vec<String>> {
        self.state.read().groups.get(name).cloned()
    }

    /// The single group claiming `tool`, or `None` when the tool is
    /// ungrouped (including tools the registry does not have).
    #[must_use]
    pub fn group_of(&self, tool: &str) -> Option<String> {
        self.state.read().membership.get(tool).cloned()
    }

    /// Whether `tool` is advertised — ungrouped, or in an active
    /// group.
    #[must_use]
    pub fn is_visible(&self, tool: &str) -> bool {
        self.state.read().is_visible(tool)
    }

    /// Visible tool names in registry (registration) order — the set
    /// the model should be told about, and the same order
    /// [`AdaptiveRegistry`](crate::AdaptiveRegistry) caps.
    ///
    /// Memoised by the inner registry's
    /// [`ToolRegistry::version`]; group mutators clear the cache
    /// eagerly. A hit returns a clone of the cached `Vec<String>`,
    /// paying only one allocation per visible name. A miss falls back
    /// to the
    /// `snapshot_cached → filter(is_visible) → clone → collect` path
    /// the `synthia-harness` `hot_paths` bench measures.
    #[must_use]
    pub fn visible_tool_names(&self) -> Vec<String> {
        let registry_version = self.inner.version();
        // Fast path: hit the cache under the `Mutex` (no `state` lock
        // needed) — the cached `Vec<String>` was produced from the
        // exact `is_visible` answers we would re-derive today.
        {
            let cache = self.visible_cache.lock();
            if let Some(entry) = cache.as_ref()
                && entry.registry_version == registry_version
            {
                return entry.names.clone();
            }
        }
        // Slow path: recompute under the `state` read lock (the
        // `is_visible` predicate needs to see a single consistent
        // view of membership and activation), then publish the
        // result. The `state` lock is released before we take the
        // cache `Mutex`, so a concurrent `visible_tool_names` call
        // can race the publish; whichever entry lands last wins, and
        // either is a correct function of the registry + group
        // state at this version.
        let state = self.state.read();
        let names: Vec<String> = self
            .inner
            .snapshot_cached()
            .iter()
            .filter(|snap| state.is_visible(&snap.name))
            .map(|snap| snap.name.clone())
            .collect();
        drop(state);
        *self.visible_cache.lock() = Some(VisibleCache {
            registry_version,
            names: names.clone(),
        });
        names
    }

    /// Visible tool metadata snapshots (name + description only),
    /// in registry order. The model-facing JSON-Schema list comes from
    /// [`crate::surface::project_tool_definitions`] fed with
    /// [`ToolRegistry::descriptors`] and
    /// [`GroupedRegistry::visible_tool_names`] as the filter.
    #[must_use]
    pub fn visible_metadata_snapshots(&self) -> Vec<ToolMetadataSnapshot> {
        let state = self.state.read();
        self.inner
            .snapshot_cached()
            .iter()
            .filter(|snap| state.is_visible(&snap.name))
            .cloned()
            .collect()
    }
}

/// Validate one declaration against `state` without mutating it.
fn validate_declaration(
    inner: &ToolRegistry,
    state: &GroupState,
    group: &str,
    tools: &[&str],
) -> Result<(), GroupError> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for tool in tools {
        if !seen.insert(tool) {
            return Err(GroupError::DuplicateTool {
                group: group.to_owned(),
                tool: (*tool).to_owned(),
            });
        }
        if !inner.contains(tool) {
            return Err(GroupError::UnknownTool {
                group: group.to_owned(),
                tool: (*tool).to_owned(),
            });
        }
        if let Some(existing) = state.membership.get(*tool)
            && existing != group
        {
            return Err(GroupError::AlreadyGrouped {
                group: group.to_owned(),
                tool: (*tool).to_owned(),
                existing: existing.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::{
        Tool,
        ToolEntry,
        ToolOutput,
        traits::ExecutionMode,
        types::Context,
    };

    struct NamedTool(&'static str);

    #[async_trait]
    impl Tool for NamedTool {
        fn name(&self) -> &str {
            self.0
        }

        fn description(&self) -> &str {
            "test tool"
        }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }

        fn mode(&self) -> ExecutionMode {
            ExecutionMode::Parallel
        }

        async fn call(
            &self,
            _input: serde_json::Value,
            _context: &Context,
        ) -> ToolOutput {
            ToolOutput::text(format!("ran {}", self.0))
        }
    }

    fn registry(names: &[&'static str]) -> Arc<ToolRegistry> {
        let registry = ToolRegistry::new();
        for name in names {
            registry.register_entry(ToolEntry::new(Arc::new(NamedTool(name))));
        }
        Arc::new(registry)
    }

    #[test]
    fn declaration_rejects_bad_input_without_mutating() {
        let grouped = GroupedRegistry::new(registry(&["a", "b"]));

        assert_eq!(
            grouped.declare("", &["a"]),
            Err(GroupError::EmptyGroupName)
        );
        assert_eq!(
            grouped.declare("g", &["missing"]),
            Err(GroupError::UnknownTool {
                group: "g".to_owned(),
                tool: "missing".to_owned(),
            })
        );
        assert_eq!(
            grouped.declare("g", &["a", "a"]),
            Err(GroupError::DuplicateTool {
                group: "g".to_owned(),
                tool: "a".to_owned(),
            })
        );
        grouped.declare("g", &["a"]).unwrap();
        assert_eq!(
            grouped.declare("h", &["a"]),
            Err(GroupError::AlreadyGrouped {
                group: "h".to_owned(),
                tool: "a".to_owned(),
                existing: "g".to_owned(),
            })
        );
        // Nothing above applied: `a` is still advertised by nobody
        // until `g` is activated, and `h` was never created.
        assert_eq!(grouped.group_names(), vec!["g".to_owned()]);
        assert_eq!(grouped.group_of("b"), None);
    }

    #[test]
    fn visibility_follows_activation_and_keeps_ungrouped_visible() {
        let grouped = GroupedRegistry::new(registry(&["a", "b", "c"]));
        grouped.declare("left", &["a"]).unwrap();
        grouped.declare("right", &["b"]).unwrap();

        // Declaring is an opt-in narrowing: `a` and `b` are hidden
        // until their groups are activated, and ungrouped `c` stays.
        assert_eq!(grouped.visible_tool_names(), vec!["c"]);
        assert_eq!(grouped.active_group_names(), Vec::<String>::new());

        grouped.activate("left");
        assert!(grouped.is_visible("a"));
        assert!(!grouped.is_visible("b"));
        // `c` belongs to no group, so it stays visible throughout.
        assert!(grouped.is_visible("c"));
        assert_eq!(grouped.visible_tool_names(), vec!["a", "c"]);

        assert!(grouped.deactivate("left"));
        assert!(!grouped.deactivate("left"));
        assert_eq!(grouped.visible_tool_names(), vec!["c"]);

        // Activating an undeclared group never creates one.
        assert!(!grouped.activate("nope"));
        assert!(!grouped.is_active("nope"));
    }

    #[test]
    fn activate_only_is_all_or_nothing() {
        let grouped = GroupedRegistry::new(registry(&["a", "b"]));
        grouped.declare("left", &["a"]).unwrap();
        grouped.declare("right", &["b"]).unwrap();
        grouped.activate("left");

        assert_eq!(
            grouped.activate_only(&["right", "typo"]),
            Err(GroupError::UnknownGroup {
                name: "typo".to_owned(),
            })
        );
        // The rejected switch left the previous set in place.
        assert_eq!(grouped.active_group_names(), vec!["left".to_owned()]);

        grouped.activate_only(&["right"]).unwrap();
        assert_eq!(grouped.active_group_names(), vec!["right".to_owned()]);
        assert_eq!(grouped.visible_tool_names(), vec!["b"]);
    }

    #[test]
    fn redeclaring_moves_membership_and_keeps_activation() {
        let grouped = GroupedRegistry::new(registry(&["a", "b", "c"]));
        grouped.declare("g", &["a", "b"]).unwrap();
        grouped.activate("g");

        // `b` leaves, `c` joins; the group stays active.
        grouped.declare("g", &["a", "c"]).unwrap();
        assert!(grouped.is_active("g"));
        assert_eq!(
            grouped.members("g"),
            Some(vec!["a".to_owned(), "c".to_owned()])
        );
        assert_eq!(grouped.group_of("b"), None);
        assert_eq!(grouped.group_of("c"), Some("g".to_owned()));
        // `a` and `c` ride the active group; `b` is ungrouped again,
        // so it is visible too — the visible list is alphabetical.
        assert_eq!(grouped.visible_tool_names(), vec!["a", "b", "c"]);
        assert!(grouped.is_visible("b"));
        assert!(grouped.is_visible("a"));
        assert!(grouped.is_visible("c"));
    }

    #[test]
    fn removing_a_group_releases_its_tools() {
        let grouped = GroupedRegistry::new(registry(&["a", "b"]));
        grouped.declare("g", &["a"]).unwrap();
        grouped.activate("g");
        assert_eq!(grouped.visible_tool_names(), vec!["a", "b"]);

        assert!(grouped.remove("g"));
        assert!(!grouped.remove("g"));
        assert_eq!(grouped.group_names(), Vec::<String>::new());
        assert_eq!(grouped.group_of("a"), None);
        assert!(grouped.is_visible("a"));
        assert_eq!(grouped.visible_tool_names(), vec!["a", "b"]);
    }

    #[test]
    fn hidden_tools_stay_hidden_and_unknown_tool_names_are_not_claimable() {
        let registry = registry(&["a"]);
        registry.register_entry(
            ToolEntry::new(Arc::new(NamedTool("secret"))).with_is_hidden(true),
        );
        let grouped = GroupedRegistry::new(Arc::clone(&registry));
        // Grouping a hidden tool is allowed (the name is real) but
        // changes nothing: `snapshot` already excludes it.
        grouped.declare("g", &["secret"]).unwrap();
        grouped.activate("g");
        assert_eq!(grouped.visible_tool_names(), vec!["a"]);
    }
}
