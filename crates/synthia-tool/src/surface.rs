//! The model-facing tool surface: one pure projection from registered
//! [`ToolDescriptor`]s to the [`ToolDefinition`]s an LLM request
//! carries.
//!
//! A registry answers two different questions — *what can run?* and
//! *what is the model told about?* — and they must not be conflated.
//! [`ToolRegistry`] owns the first; this module
//! owns the second. Nothing here mutates state, so a projection is
//! replay-stable: the same descriptors + transcript + filter always
//! yield the same list, in the same order.
//!
//! ## Exposure rules
//!
//! For each descriptor that survives the visibility filter:
//!
//! | exposure | in the transcript | advertised as |
//! |---|---|---|
//! | `Direct` | either | name + description + full `parameters()` schema |
//! | `Deferred` | no | name + description + open placeholder schema |
//! | `Deferred` | yes | name + description + full `parameters()` schema |
//! | `Hidden` | either | nothing |
//!
//! `is_hidden` (the registry's privacy flag) removes a tool from the
//! projection regardless of its exposure; unlike
//! [`ToolExposure::Hidden`] it *also* makes
//! [`ToolRegistry::run_stream`](crate::ToolRegistry::run_stream)
//! refuse the tool.
//!
//! A `Deferred` tool is advertised with
//! `{"type":"object","additionalProperties":true}` instead of its real
//! schema: the model can call it from the description alone, and the
//! tool's own argument validation still runs on dispatch. This is a
//! deliberate divergence from pi
//! (`packages/ai/src/utils/deferred-tools.ts`), which drops deferred
//! tools from the advertised list entirely and relies on the
//! transcript mentioning their names — a provider that rejects an
//! unadvertised tool name would turn a prompt-economy choice into a
//! hard failure.
//!
//! ## Promotion is transcript-derived
//!
//! [`called_tool_names`] scans the messages that will be sent and
//! returns every tool name they mention, so a `Deferred` tool promotes
//! itself on the next request after its first call without the
//! registry — or any other mutable state — tracking anything.
//!
//! ## Composing the visible-name filter
//!
//! [`project_tool_definitions`] takes an optional set of visible
//! names, which lets the tier cap and the groups seam reach the same
//! projection:
//!
//! ```
//! # use std::{collections::HashSet, sync::Arc};
//! # use synthia_provider::ModelTier;
//! # use synthia_tool::{
//! #     AdaptiveRegistry, ToolDescriptor, ToolRegistry,
//! #     called_tool_names, project_tool_definitions,
//! # };
//! # let registry = Arc::new(ToolRegistry::new());
//! # let messages: Vec<synthia_provider::Message> = Vec::new();
//! let adaptive =
//!     AdaptiveRegistry::new(Arc::clone(&registry), ModelTier::Small);
//! let visible: HashSet<String> =
//!     adaptive.visible_tool_names().into_iter().collect();
//! let defs = project_tool_definitions(
//!     &registry.descriptors(),
//!     &called_tool_names(&messages),
//!     Some(&visible),
//! );
//! ```
//!
//! [`GroupedRegistry`](crate::GroupedRegistry) composes identically
//! through its own `visible_tool_names()`. Passing `None` advertises
//! every non-hidden tool.
//!
//! ## Deployment-level policy
//!
//! [`ToolSurfacePolicy`] is the serializable twin of those two
//! wrappers: a deployment declares groups, an active set, and a cap
//! once. [`ToolSurfacePolicy::apply`] writes the group verdict onto
//! the registry's [`ToolExposure`]s, while
//! [`ToolSurfacePolicy::visible_tool_names`] is the allow-list half
//! the projection takes — so a config file reaches the same seam as
//! the runtime wrappers.

use std::collections::{BTreeMap, HashSet};

use synthia_provider::{ContentPart, Message, ToolDefinition};
use thiserror::Error;

use crate::{
    ToolRegistry,
    registry::{ToolDescriptor, ToolExposure},
};

/// The schema advertised for a `Deferred` tool before its first call.
///
/// Deliberately permissive: the model chooses arguments from the tool's
/// description, and the tool validates them itself on dispatch.
fn placeholder_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": true,
    })
}

/// Project `descriptors` into the model-facing tool list.
///
/// Ordering follows `descriptors`; [`ToolRegistry::descriptors`](crate::ToolRegistry::descriptors)
/// sorts by name for a stable list. `called` is the set of tool names
/// the transcript already mentions (see [`called_tool_names`]) and
/// drives `Deferred` promotion. `visible` is an optional allow-list —
/// `None` advertises every non-hidden tool.
///
/// See the module docs for the exposure rules and how to compose the
/// filter with [`AdaptiveRegistry`](crate::AdaptiveRegistry) or
/// [`GroupedRegistry`](crate::GroupedRegistry).
#[must_use]
pub fn project_tool_definitions(
    descriptors: &[ToolDescriptor],
    called: &HashSet<String>,
    visible: Option<&HashSet<String>>,
) -> Vec<ToolDefinition> {
    let mut defs = Vec::with_capacity(descriptors.len());
    for descriptor in descriptors {
        if !is_advertised(descriptor, visible) {
            continue;
        }
        defs.push(definition_for(descriptor, called));
    }
    defs
}

/// Whether `descriptor` reaches the model at all: not hidden, exposure
/// not `Hidden`, and (when a filter is given) named by it.
fn is_advertised(
    descriptor: &ToolDescriptor,
    visible: Option<&HashSet<String>>,
) -> bool {
    if descriptor.is_hidden || descriptor.exposure == ToolExposure::Hidden {
        return false;
    }
    visible
        .map(|names| names.contains(&descriptor.name))
        .unwrap_or(true)
}

/// The definition one advertised descriptor contributes: full schema,
/// except for a not-yet-called `Deferred` tool, which gets the
/// placeholder.
fn definition_for(
    descriptor: &ToolDescriptor,
    called: &HashSet<String>,
) -> ToolDefinition {
    let schema = match descriptor.exposure {
        ToolExposure::Deferred if !called.contains(&descriptor.name) => {
            placeholder_schema()
        }
        _ => descriptor.parameters.clone(),
    };
    let mut def = ToolDefinition::new(
        descriptor.name.clone(),
        descriptor.description.clone(),
        schema,
    );
    if let Some(annotations) = descriptor.annotations {
        def.annotations = Some(annotations.into());
    }
    def
}

/// Every tool name the transcript mentions: `ToolUse` names (the
/// assistant asking for a tool) plus `ToolResult` names (a result
/// injected or replayed without its matching call, e.g. from a session
/// log).
///
/// The scan is by content shape, not role, so it survives a replayed
/// history whose roles were rewritten. O(messages × parts), no state:
/// promotion needs no mutable registry bookkeeping.
#[must_use]
pub fn called_tool_names(messages: &[Message]) -> HashSet<String> {
    let mut names = HashSet::new();
    for message in messages {
        for part in &message.content {
            match part {
                ContentPart::ToolUse(call) => {
                    names.insert(call.name.clone());
                }
                ContentPart::ToolResult(result) => {
                    if let Some(name) = result.tool_name.as_ref() {
                        names.insert(name.clone());
                    }
                }
                _ => {}
            }
        }
    }
    names
}

/// Why a [`ToolSurfacePolicy`] was rejected.
///
/// Mirrors [`GroupError`](crate::GroupError): every variant names the
/// group and tool involved, so a deployment wiring a catalog finds the
/// typo without guessing.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SurfacePolicyError {
    /// A group must have a name; an empty one cannot be activated or
    /// logged meaningfully.
    #[error("group name must not be empty")]
    EmptyGroupName,
    /// [`ToolSurfacePolicy::active_groups`] names a group that was
    /// never declared in [`ToolSurfacePolicy::groups`].
    #[error("active group `{name}` was never declared")]
    UnknownGroup {
        /// Undeclared group name.
        name: String,
    },
    /// A group names a tool the registry does not have.
    #[error("group `{group}` names tool `{tool}`, which is not registered")]
    UnknownTool {
        /// Group being declared.
        group: String,
        /// Unregistered tool name.
        tool: String,
    },
    /// The same tool appears twice in one group.
    #[error("group `{group}` names tool `{tool}` twice")]
    DuplicateTool {
        /// Group being declared.
        group: String,
        /// Repeated tool name.
        tool: String,
    },
    /// The tool is claimed by a different group, so this one cannot
    /// also claim it.
    #[error(
        "tool `{tool}` is already in group `{existing}`, so group \
         `{group}` cannot claim it"
    )]
    AlreadyGrouped {
        /// Group being declared.
        group: String,
        /// Contested tool name.
        tool: String,
        /// Group that already claims it — the earlier group in
        /// `BTreeMap` order, so the error is deterministic.
        existing: String,
    },
}

/// A deployment-level description of the model-facing tool surface.
///
/// [`GroupedRegistry`](crate::GroupedRegistry) and
/// [`AdaptiveRegistry`](crate::AdaptiveRegistry) expose the same two
/// levers — named groups with an active set, and a cap on how many
/// tools the model is told about — as *runtime* wrappers over a
/// registry. This type is their serializable twin: a deployment
/// declares the surface once, [`ToolSurfacePolicy::apply`] writes the
/// group verdict onto the registry's [`ToolExposure`]s, and
/// [`ToolSurfacePolicy::visible_tool_names`] produces the allow-list
/// [`project_tool_definitions`] filters with.
///
/// ## Group rule
///
/// | tool's group | [`apply`](Self::apply) writes | advertised |
/// |---|---|---|
/// | active | [`ToolExposure::Direct`] | yes |
/// | inactive | [`ToolExposure::Hidden`] | no |
/// | none | untouched | as registered (`Direct` by default) |
///
/// Inactive members become `Hidden` *exposure* — not
/// [`ToolRegistry::set_hidden`]'s privacy flag — so they stay
/// dispatcheable. Narrowing what the model sees is a prompt-economy
/// decision, never a security boundary: the contract
/// [`GroupedRegistry`](crate::GroupedRegistry) already documents.
/// This rule makes the policy verdict-identical to that wrapper —
/// same declarations, same active set, same model-facing list.
///
/// [`apply`](Self::apply) writes **only** tools named by a group; a
/// tool in no group keeps whatever exposure it was registered with.
/// That keeps the policy composable with registration-time
/// `with_exposure` and with other writers (e.g. a server's
/// `deferred`/`hidden` lists).
///
/// ## The cap is the caller's
///
/// `max_visible` is deliberately *not* applied by
/// [`apply`](Self::apply): it belongs to one request, not to the
/// registry — the same catalog may be shown capped to a small model
/// and uncapped to a large one. [`visible_tool_names`](Self::visible_tool_names)
/// is the caller-side half: it returns the capped allow-list, and the
/// caller hands it to [`project_tool_definitions`]. Tools the
/// registry already hides (`is_hidden`, or [`ToolExposure::Hidden`])
/// take no cap slot, matching [`AdaptiveRegistry`](crate::AdaptiveRegistry),
/// which caps after [`ToolRegistry::snapshot`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolSurfacePolicy {
    /// Cap on advertised tools, or `None` for the whole catalog.
    /// Applied by [`ToolSurfacePolicy::visible_tool_names`], not by
    /// [`ToolSurfacePolicy::apply`].
    pub max_visible: Option<usize>,
    /// Declared groups: group name → member tool names, in
    /// declaration order. A `BTreeMap`, so iteration (and therefore
    /// every error and write) is name-sorted and replay-stable.
    pub groups: BTreeMap<String, Vec<String>>,
    /// Groups whose members are advertised. Every name must be a
    /// declared group.
    pub active_groups: Vec<String>,
}

impl ToolSurfacePolicy {
    /// Whether `group` is listed in [`Self::active_groups`].
    #[must_use]
    pub fn is_active(&self, group: &str) -> bool {
        self.active_groups.iter().any(|name| name == group)
    }

    /// Write this policy's group verdict onto `registry`.
    ///
    /// Every member must be registered and claimed by exactly one
    /// group, every active group must be declared, and group names
    /// must be non-empty. Validation completes before the first
    /// write, so a rejected policy changes nothing.
    ///
    /// # Errors
    ///
    /// [`SurfacePolicyError`] naming the offending group and tool.
    pub fn apply(
        &self,
        registry: &ToolRegistry,
    ) -> Result<(), SurfacePolicyError> {
        self.validate(registry)?;
        for (group, tools) in &self.groups {
            let exposure = if self.is_active(group) {
                ToolExposure::Direct
            } else {
                ToolExposure::Hidden
            };
            for tool in tools {
                // Validated above; `false` here would mean a
                // concurrent unregister between validation and
                // write, which leaves the registry consistent.
                if !registry.set_exposure(tool, exposure) {
                    tracing::warn!(
                        group = %group,
                        tool = %tool,
                        "tool surface policy: tool vanished during apply"
                    );
                }
            }
        }
        Ok(())
    }

    /// Validate the whole policy against `registry` without writing.
    fn validate(
        &self,
        registry: &ToolRegistry,
    ) -> Result<(), SurfacePolicyError> {
        // tool → owning group, filled in first-declaration order.
        let mut owner: BTreeMap<&str, &str> = BTreeMap::new();
        for (group, tools) in &self.groups {
            if group.is_empty() {
                return Err(SurfacePolicyError::EmptyGroupName);
            }
            for tool in tools {
                if !registry.contains(tool) {
                    return Err(SurfacePolicyError::UnknownTool {
                        group: group.clone(),
                        tool: tool.clone(),
                    });
                }
                match owner.insert(tool.as_str(), group.as_str()) {
                    None => {}
                    Some(previous) if previous == group => {
                        return Err(SurfacePolicyError::DuplicateTool {
                            group: group.clone(),
                            tool: tool.clone(),
                        });
                    }
                    Some(previous) => {
                        return Err(SurfacePolicyError::AlreadyGrouped {
                            group: group.clone(),
                            tool: tool.clone(),
                            existing: previous.to_string(),
                        });
                    }
                }
            }
        }
        for name in &self.active_groups {
            if !self.groups.contains_key(name) {
                return Err(SurfacePolicyError::UnknownGroup {
                    name: name.clone(),
                });
            }
        }
        Ok(())
    }

    /// The allow-list [`project_tool_definitions`] should filter with:
    /// every groupless tool, every member of an active group, and —
    /// when [`Self::max_visible`] is set — only the first N of them in
    /// `descriptors` order.
    ///
    /// `descriptors` is [`ToolRegistry::descriptors`]' output, whose
    /// name-sorted order makes the cap deterministic (a `HashSet` is
    /// returned, so the caller's list order still comes from
    /// `descriptors`, not from insertion order here). Already-hidden
    /// descriptors take no cap slot.
    ///
    /// This read side is deliberately lenient — a name that is not in
    /// `descriptors` simply cannot match, and a duplicate claim is
    /// resolved as "any active claimant wins" — so an agent loop can
    /// apply a policy without an error path on the hot path.
    /// [`ToolSurfacePolicy::apply`] is the strict validator.
    #[must_use]
    pub fn visible_tool_names(
        &self,
        descriptors: &[ToolDescriptor],
    ) -> HashSet<String> {
        let mut names = HashSet::new();
        let mut budget = self.max_visible;
        for descriptor in descriptors {
            if descriptor.is_hidden
                || descriptor.exposure == ToolExposure::Hidden
                || !self.admits(&descriptor.name)
            {
                continue;
            }
            match budget {
                Some(0) => break,
                Some(remaining) => budget = Some(remaining - 1),
                None => {}
            }
            names.insert(descriptor.name.clone());
        }
        names
    }

    /// Whether the group rule admits `tool`: groupless tools and
    /// members of active groups are admitted; a member of only
    /// inactive groups is not.
    fn admits(&self, tool: &str) -> bool {
        let mut claimed = false;
        for (group, members) in &self.groups {
            if members.iter().any(|member| member == tool) {
                if self.is_active(group) {
                    return true;
                }
                claimed = true;
            }
        }
        !claimed
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use synthia_provider::{Content, ContentPart, Role, ToolResult, ToolUse};

    use super::*;
    use crate::registry::ToolCategory;

    fn descriptor(name: &str, exposure: ToolExposure) -> ToolDescriptor {
        ToolDescriptor {
            name: name.to_string(),
            description: format!("the {name} tool"),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"],
            }),
            category: ToolCategory::Utility,
            is_hidden: false,
            exposure,
            annotations: None,
        }
    }

    fn names(defs: &[ToolDefinition]) -> Vec<&str> {
        defs.iter().map(|d| d.name.as_str()).collect()
    }

    /// Assistant message asking for `names`, which is how a call shows
    /// up in the transcript.
    fn assistant_call(names: &[&str]) -> Message {
        let parts: Vec<ContentPart> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                ContentPart::ToolUse(ToolUse {
                    id: format!("call-{i}"),
                    name: (*name).to_string(),
                    input: serde_json::json!({}),
                })
            })
            .collect();
        Message::new(Role::Assistant, Content::parts(parts))
    }

    /// Tool-role message whose `ToolResult` carries the tool name.
    fn tool_result(name: Option<&str>) -> Message {
        let mut result = ToolResult::new("call-0", "ok");
        result.tool_name = name.map(str::to_string);
        Message::new(
            Role::Tool,
            Content::Single(ContentPart::ToolResult(result)),
        )
    }

    #[test]
    fn direct_tool_definition_is_exactly_name_description_and_schema() {
        let descs = vec![descriptor("read_file", ToolExposure::Direct)];
        let defs = project_tool_definitions(&descs, &HashSet::new(), None);
        assert_eq!(
            serde_json::to_value(&defs).unwrap(),
            serde_json::json!([{
                "name": "read_file",
                "description": "the read_file tool",
                "input_schema": {
                    "type": "object",
                    "properties": {"path": {"type": "string"}},
                    "required": ["path"],
                },
            }]),
            "Direct must advertise the tool verbatim"
        );
    }

    #[test]
    fn deferred_tool_is_promoted_by_a_called_name() {
        let descs = vec![descriptor("query_db", ToolExposure::Deferred)];
        let before = project_tool_definitions(&descs, &HashSet::new(), None);
        assert_eq!(names(&before), vec!["query_db"]);
        assert_eq!(
            before[0].description, "the query_db tool",
            "the description is what lets the model call a deferred tool"
        );
        assert_eq!(
            before[0].input_schema,
            serde_json::json!({
                "type": "object",
                "additionalProperties": true,
            }),
            "an uncalled Deferred tool must not leak its real schema"
        );

        let called: HashSet<String> =
            ["query_db".to_string()].into_iter().collect();
        let after = project_tool_definitions(&descs, &called, None);
        assert_eq!(
            after[0].input_schema, descs[0].parameters,
            "after its first call the Deferred tool carries its full schema"
        );
    }

    #[test]
    fn called_name_promotes_only_the_tool_it_names() {
        let descs = vec![
            descriptor("alpha", ToolExposure::Deferred),
            descriptor("beta", ToolExposure::Deferred),
        ];
        let called: HashSet<String> =
            ["alpha".to_string()].into_iter().collect();
        let defs = project_tool_definitions(&descs, &called, None);
        assert_eq!(defs[0].input_schema, descs[0].parameters);
        assert_eq!(
            defs[1].input_schema,
            serde_json::json!({
                "type": "object",
                "additionalProperties": true,
            })
        );
    }

    #[test]
    fn hidden_tools_are_absent_from_every_projection() {
        let mut flagged = descriptor("secret", ToolExposure::Direct);
        flagged.is_hidden = true;
        let descs = vec![
            flagged,
            descriptor("internal", ToolExposure::Hidden),
            descriptor("visible", ToolExposure::Direct),
        ];
        // Even a called name must not resurrect a hidden tool.
        let called: HashSet<String> =
            ["secret".to_string(), "internal".to_string()]
                .into_iter()
                .collect();
        let defs = project_tool_definitions(&descs, &called, None);
        assert_eq!(names(&defs), vec!["visible"]);
    }

    #[test]
    fn called_tool_names_reads_tool_use_and_tool_result_names() {
        let messages = vec![
            Message::user("hello"),
            assistant_call(&["read_file", "query_db"]),
            tool_result(Some("query_db")),
            tool_result(None),
            Message::assistant("thinking"),
        ];
        let called = called_tool_names(&messages);
        let mut sorted: Vec<&str> = called.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        assert_eq!(sorted, vec!["query_db", "read_file"]);
    }

    #[test]
    fn called_tool_names_is_empty_for_a_transcript_without_calls() {
        let messages = vec![Message::user("hi"), Message::assistant("hello")];
        assert!(called_tool_names(&messages).is_empty());
    }

    #[test]
    fn visible_filter_narrows_the_projected_list() {
        let descs = vec![
            descriptor("alpha", ToolExposure::Direct),
            descriptor("beta", ToolExposure::Direct),
            descriptor("gamma", ToolExposure::Direct),
        ];
        let visible: HashSet<String> =
            ["alpha".to_string(), "gamma".to_string()]
                .into_iter()
                .collect();
        let defs =
            project_tool_definitions(&descs, &HashSet::new(), Some(&visible));
        assert_eq!(names(&defs), vec!["alpha", "gamma"]);

        // The filter is an allow-list: it also withholds a Deferred
        // tool that the transcript has already promoted.
        let descs = vec![
            descriptor("alpha", ToolExposure::Direct),
            descriptor("beta", ToolExposure::Deferred),
        ];
        let called: HashSet<String> =
            ["beta".to_string()].into_iter().collect();
        let visible: HashSet<String> =
            ["alpha".to_string()].into_iter().collect();
        let defs = project_tool_definitions(&descs, &called, Some(&visible));
        assert_eq!(names(&defs), vec!["alpha"]);
    }

    /// The regression contract end to end: a real registry whose tools
    /// are all `Direct` and not hidden produces exactly the definitions
    /// the pre-projection call sites built from `list(None)` +
    /// `Tool::parameters()` — even when the transcript has already
    /// mentioned them. Element *contents* must match byte-for-byte;
    /// the projection only reorders them (sorted by name, per
    /// `ToolRegistry::descriptors`), which is why both sides are sorted
    /// before comparing.
    #[tokio::test]
    async fn registry_with_only_direct_tools_matches_the_legacy_list() {
        use async_trait::async_trait;

        use crate::{
            Tool,
            ToolEntry,
            ToolOutput,
            ToolRegistry,
            types::Context,
        };

        struct SchemaTool(&'static str);

        #[async_trait]
        impl Tool for SchemaTool {
            fn name(&self) -> &str {
                self.0
            }

            fn description(&self) -> &str {
                "a tool with a real schema"
            }

            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({
                    "type": "object",
                    "properties": {"arg": {"type": "string"}},
                    "required": ["arg"],
                })
            }

            async fn call(
                &self,
                _input: serde_json::Value,
                _context: &Context,
            ) -> ToolOutput {
                ToolOutput::text("ok")
            }
        }

        let registry = ToolRegistry::new();
        for name in ["beta", "alpha"] {
            registry.register_entry(ToolEntry::new(Arc::new(SchemaTool(name))));
        }

        let mut legacy: Vec<serde_json::Value> =
            synthia_core::registry::Registry::list(&registry, None)
                .await
                .expect("in-memory registry lists")
                .into_iter()
                .map(|entry| {
                    let tool = entry.tool_instance();
                    serde_json::to_value(ToolDefinition::new(
                        tool.name(),
                        tool.description(),
                        tool.parameters(),
                    ))
                    .expect("definitions are JSON values")
                })
                .collect();
        let mut projected: Vec<serde_json::Value> = project_tool_definitions(
            &registry.descriptors(),
            &called_tool_names(&[assistant_call(&["alpha", "beta"])]),
            None,
        )
        .iter()
        .map(|def| {
            serde_json::to_value(def).expect("definitions are JSON values")
        })
        .collect();
        let by_name = |a: &serde_json::Value, b: &serde_json::Value| {
            a["name"].as_str().cmp(&b["name"].as_str())
        };
        legacy.sort_by(by_name);
        projected.sort_by(by_name);

        assert_eq!(
            projected, legacy,
            "a Direct-only registry must project to the legacy definitions"
        );
        assert_eq!(projected.len(), 2);
    }

    // -- ToolSurfacePolicy -------------------------------------------

    /// A registry with `names` registered in the given order, each
    /// carrying a per-tool schema so a projection mix-up is visible.
    fn registry_with(names: &[&str]) -> crate::ToolRegistry {
        let registry = crate::ToolRegistry::new();
        for name in names {
            registry.register_entry(crate::ToolEntry::dynamic(
                (*name).to_string(),
                format!("the {name} tool"),
                serde_json::json!({
                    "type": "object",
                    "properties": {"for": {"const": name}},
                }),
            ));
        }
        registry
    }

    fn policy(
        groups: &[(&str, &[&str])],
        active: &[&str],
        max_visible: Option<usize>,
    ) -> ToolSurfacePolicy {
        ToolSurfacePolicy {
            max_visible,
            groups: groups
                .iter()
                .map(|(group, tools)| {
                    (
                        (*group).to_string(),
                        tools.iter().map(|t| (*t).to_string()).collect(),
                    )
                })
                .collect(),
            active_groups: active.iter().map(|g| (*g).to_string()).collect(),
        }
    }

    fn sorted_names(names: &HashSet<String>) -> Vec<&str> {
        let mut out: Vec<&str> = names.iter().map(String::as_str).collect();
        out.sort_unstable();
        out
    }

    /// `apply` writes the documented verdict — active group members
    /// `Direct`, inactive members `Hidden` exposure, groupless tools
    /// untouched — and the projection then advertises exactly the
    /// policy's visible set.
    #[test]
    fn policy_apply_writes_the_group_verdict_the_projection_reads() {
        let registry = registry_with(&["read", "write", "shell", "free"]);
        let policy = policy(
            &[("files", &["read", "write"]), ("ops", &["shell"])],
            &["files"],
            None,
        );

        policy.apply(&registry).expect("valid policy");

        assert_eq!(registry.exposure("read"), Some(ToolExposure::Direct));
        assert_eq!(registry.exposure("write"), Some(ToolExposure::Direct));
        assert_eq!(
            registry.exposure("shell"),
            Some(ToolExposure::Hidden),
            "an inactive group member is Hidden exposure"
        );
        assert_eq!(
            registry.exposure("free"),
            Some(ToolExposure::Direct),
            "a groupless tool keeps its registered exposure"
        );

        let descriptors = registry.descriptors();
        assert_eq!(
            names(&project_tool_definitions(
                &descriptors,
                &HashSet::new(),
                Some(&policy.visible_tool_names(&descriptors)),
            )),
            vec!["free", "read", "write"],
            "the model list must follow the group verdict, sorted by name"
        );
        assert!(
            registry.contains("shell"),
            "Hidden exposure must not remove the tool from dispatch"
        );
    }

    /// A policy that names an unregistered tool, claims one tool from
    /// two groups, repeats one inside a group, or activates a group it
    /// never declared is a typed error — and each rejection leaves the
    /// registry exactly as it was.
    #[test]
    fn invalid_policies_are_typed_errors_that_change_nothing() {
        let registry = registry_with(&["read"]);
        registry.set_exposure("read", ToolExposure::Deferred);
        let version = registry.version();

        let unknown =
            policy(&[("files", &["read", "ghost"])], &["files"], None);
        assert_eq!(
            unknown.apply(&registry),
            Err(SurfacePolicyError::UnknownTool {
                group: "files".to_string(),
                tool: "ghost".to_string(),
            })
        );

        let twice_claimed =
            policy(&[("a", &["read"]), ("b", &["read"])], &[], None);
        assert_eq!(
            twice_claimed.apply(&registry),
            Err(SurfacePolicyError::AlreadyGrouped {
                group: "b".to_string(),
                tool: "read".to_string(),
                existing: "a".to_string(),
            }),
            "BTreeMap order makes the reported owner deterministic"
        );

        let repeated = policy(&[("a", &["read", "read"])], &["a"], None);
        assert_eq!(
            repeated.apply(&registry),
            Err(SurfacePolicyError::DuplicateTool {
                group: "a".to_string(),
                tool: "read".to_string(),
            })
        );

        let unknown_group = policy(&[("a", &["read"])], &["nope"], None);
        assert_eq!(
            unknown_group.apply(&registry),
            Err(SurfacePolicyError::UnknownGroup {
                name: "nope".to_string(),
            })
        );

        assert_eq!(
            registry.exposure("read"),
            Some(ToolExposure::Deferred),
            "a rejected policy must not have written anything"
        );
        assert_eq!(registry.version(), version);
    }

    /// The cap takes the first N *advertisable* descriptors in
    /// registry (name) order; tools already hidden by `is_hidden` or
    /// `Hidden` exposure take no slot.
    #[test]
    fn policy_visible_names_cap_is_registry_order_and_skips_hidden() {
        let registry = registry_with(&["alpha", "beta", "delta", "gamma"]);
        registry.set_hidden("beta", true);
        registry.set_exposure("gamma", ToolExposure::Hidden);
        let descriptors = registry.descriptors();

        let uncapped = policy(&[], &[], None);
        assert_eq!(
            sorted_names(&uncapped.visible_tool_names(&descriptors)),
            vec!["alpha", "delta"],
            "hidden tools never reach the allow-list"
        );

        let capped = policy(&[], &[], Some(1));
        assert_eq!(
            sorted_names(&capped.visible_tool_names(&descriptors)),
            vec!["alpha"],
            "the cap takes the first advertisable name in registry order"
        );

        let wide = policy(&[], &[], Some(9));
        assert_eq!(
            sorted_names(&wide.visible_tool_names(&descriptors)),
            vec!["alpha", "delta"],
            "a cap above the catalog changes nothing"
        );

        let narrowed = policy(&[("core", &["alpha"])], &[], Some(1));
        assert_eq!(
            sorted_names(&narrowed.visible_tool_names(&descriptors)),
            vec!["delta"],
            "the cap counts only what the group rule admits, so alpha \
             (inactive group) frees the slot"
        );
    }

    /// The documented parity: same declarations, same active set, same
    /// model-facing list as [`crate::GroupedRegistry`].
    #[test]
    fn policy_and_grouped_registry_agree_on_the_model_list() {
        let registry =
            Arc::new(registry_with(&["alpha", "beta", "gamma", "delta"]));
        let policy = policy(
            &[("core", &["alpha", "beta"]), ("ops", &["gamma"])],
            &["core"],
            None,
        );
        let grouped = crate::GroupedRegistry::new(Arc::clone(&registry));
        grouped.declare("core", &["alpha", "beta"]).unwrap();
        grouped.declare("ops", &["gamma"]).unwrap();
        grouped.activate("core");
        policy.apply(&registry).expect("valid policy");

        let descriptors = registry.descriptors();
        let from_policy: Vec<String> = project_tool_definitions(
            &descriptors,
            &HashSet::new(),
            Some(&policy.visible_tool_names(&descriptors)),
        )
        .into_iter()
        .map(|d| d.name)
        .collect();
        assert_eq!(from_policy, grouped.visible_tool_names());
        assert_eq!(from_policy, vec!["alpha", "beta", "delta"]);
    }
}
