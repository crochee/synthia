use std::{collections::HashSet, sync::Arc};

use axum::{Json, extract::State};
use serde::Serialize;
use synthia::core::{
    Error,
    registry::{Registry, RegistryItem},
};

use super::helpers::paginate;
use crate::{
    api::{
        AppError,
        AppJson,
        AppPath,
        AppQuery,
        List,
        PageQuery,
        resolve_page,
        validate_resource_name,
        validate_sort,
    },
    state::AppState,
};

/// Sortable fields for the tools list endpoint.
const TOOL_SORT_WHITELIST: &[&str] = &["name"];

#[derive(Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct ToolDetail {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    /// `core` for tools compiled into the binary, `dynamic`
    /// for tools registered at runtime.
    pub provenance: String,
    /// R17: the tool's canonical rendering contract
    /// (`RenderKind` + title + presentation hints). Consumers
    /// (UI, replay) pick a renderer from this instead of
    /// hard-coding per-tool shapes.
    pub output_definition: synthia::tool::ToolOutputDefinition,
}

/// Collect tool definitions from the registry, keyed by the
/// registry's monotonic version. Hit: Arc clone (8-byte
/// refcount) + full Vec deep clone for the caller (the schema
/// `serde_json::Value` clones are unavoidable but only happen on
/// the consumer side, not the registry side). Miss: acquire
/// the registry read lock, walk the descriptors, project them
/// through [`synthia::tool::project_tool_definitions`], store
/// under the current version.
///
/// `ToolDefinition::input_schema` is built from `parameters()`,
/// which (for built-in tools) calls `schemars::schema_for!` and
/// `serde_json::to_value` on every invocation — tens of
/// microseconds per tool × N tools per request — so the projected
/// list is cached on the [`AppState`] keyed by the registry's
/// version. Every registry mutation that changes the model-facing
/// list (register, unregister, [`synthia::tool::ToolRegistry::set_exposure`],
/// [`synthia::tool::ToolRegistry::set_hidden`]) bumps that version,
/// so a cache hit only ever serves the same registry at the same
/// version. The cache lives on the state (not a process-wide
/// static) so two servers in one process cannot read each other's
/// list.
///
/// The projection is the same seam the agent loop uses, so this
/// endpoint reports what the model would be offered: `Direct` tools
/// with their full schema, `Deferred` tools with the placeholder
/// until a call promotes them, and `Hidden` / `is_hidden` tools
/// nowhere. There is no transcript here, so no tool is ever
/// promoted on this path; `max_visible` is likewise not applied —
/// the cap belongs to one agent request, not to the catalog.
async fn collect_tool_defs(
    state: &Arc<AppState>,
) -> Vec<synthia::provider::ToolDefinition> {
    let version = state.tool_registry.read().await.version();
    {
        let guard = state.tool_defs_cache.read();
        if let Some((v, cached)) = guard.as_ref()
            && *v == version
        {
            return (**cached).clone();
        }
    }
    let descriptors = state.tool_registry.read().await.descriptors();
    // Self-management MCP tools (`mcp__self__*`) are server-facing
    // (operators reach them via MCP), not model-facing. The HTTP
    // `/api/tools` catalog follows the same contract as the agent
    // loop's `projected_tool_definitions`: hide them so an operator
    // browsing the tool surface is not presented with nine
    // server-management entries that the LLM cannot call.
    let operator_facing: Vec<synthia::tool::ToolDescriptor> = descriptors
        .into_iter()
        .filter(|d| !d.name.starts_with("mcp__self__"))
        .collect();
    let defs: Vec<synthia::provider::ToolDefinition> =
        synthia::tool::project_tool_definitions(
            &operator_facing,
            &HashSet::new(),
            None,
        );
    let arc = Arc::new(defs);
    *state.tool_defs_cache.write() = Some((version, arc.clone()));
    (*arc).clone()
}

/// GET /api/tools - List registered tools.
pub async fn list_tools(
    State(state): State<Arc<AppState>>,
    AppQuery(page): AppQuery<PageQuery>,
) -> Result<Json<List<ToolInfo>>, AppError> {
    validate_sort(page.sort.as_deref().unwrap_or("name"), TOOL_SORT_WHITELIST)?;
    let resolved = resolve_page(&page)?;

    let defs = collect_tool_defs(&state).await;
    let mut tools: Vec<ToolInfo> = defs
        .into_iter()
        .map(|d| ToolInfo {
            name: d.name,
            description: d.description,
        })
        .collect();

    tools.sort_by(|a, b| a.name.cmp(&b.name));
    if resolved.descending {
        tools.reverse();
    }

    let list = paginate(tools, &resolved, |t: &ToolInfo| t.name.as_str());
    Ok(Json(list))
}

// POST /api/tools - Register a tool.
//
// Restored in turn 13 of the 2026-08-15 optimization pass to
// address Task 3 of the active goal ("实现skill.tool、agent、
// model的全生命周期管理"). Tools live in the in-memory
// `tool_registry` and survive until the server restarts; the
// dynamic registration accepts a name + description + JSON
// schema and registers a passthrough tool that echoes its
// arguments back.
pub async fn register_tool(
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<RegisterToolRequest>,
) -> Result<Json<ToolInfo>, AppError> {
    validate_resource_name(&req.name)?;

    let desc = req.description.clone();
    let params = req.input_schema.clone();
    let tool = synthia::tool::ToolEntry::dynamic(
        req.name.clone(),
        desc.clone(),
        params,
    );

    let reg = state.tool_registry.write().await;
    let existed = reg
        .list(None)
        .await
        .map(|entries| entries.iter().any(|e| e.name() == req.name))
        .unwrap_or(false);
    if existed {
        return Err(AppError::from(Error::already_exists(format!(
            "tool '{}'",
            req.name
        ))));
    }
    reg.put(tool).await.map_err(|e| {
        Error::internal(format!("failed to register tool: {e}"))
    })?;

    Ok(Json(ToolInfo {
        name: req.name,
        description: desc,
    }))
}

/// Request body for `POST /api/v1/tools`.
#[derive(serde::Deserialize, validator::Validate)]
pub struct RegisterToolRequest {
    #[validate(length(min = 1, message = "must not be empty"))]
    pub name: String,
    #[validate(length(min = 1, message = "must not be empty"))]
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// GET /api/tools/{name} - Get a single tool.
pub async fn get_tool(
    State(state): State<Arc<AppState>>,
    AppPath(name): AppPath<String>,
) -> Result<Json<ToolDetail>, AppError> {
    validate_resource_name(&name)?;
    let tool_reg = state.tool_registry.read().await;
    // O(1) lookup against the registry's `HashMap<String, Vec<ToolEntry>>`
    // instead of the previous O(n) `list()` + linear search that
    // cloned every `ToolDefinition` (including the JSON schema)
    // just to find the one named entry. The previous path wasted
    // work on every detail-page render and the front-end's per-
    // detail-page prefetch.
    let entry = tool_reg
        .get(&name)
        .await
        .map_err(|e| Error::internal(e.to_string()))?;
    let entry = entry.ok_or_else(|| {
        AppError::from(Error::not_found(format!("tool '{name}'")))
    })?;
    // Provenance isn't on `ToolEntry`; the registry exposes it via
    // a snapshot scan. The snapshot is cheap (one read-lock, single
    // pass over an in-memory `HashMap`) and much smaller than the
    // full `list()` payload the previous code built — we just need
    // the one matching record.
    let snap = tool_reg.snapshot_with_provenance();
    let provenance = snap
        .iter()
        .find(|r| r.metadata.name == name)
        .map(|r| match r.provenance {
            synthia::tool::registry::ToolProvenance::Core => "core",
            synthia::tool::registry::ToolProvenance::Dynamic => "dynamic",
        })
        .unwrap_or("dynamic")
        .to_string();
    let description = entry.description().to_string();
    let input_schema = entry.tool_instance().parameters();
    let output_definition = entry.tool_instance().output_definition();
    Ok(Json(ToolDetail {
        name: entry.name().to_string(),
        description,
        input_schema,
        provenance,
        output_definition,
    }))
}

// DELETE /api/tools/{name} - Unregister a tool.
//
// Restored in turn 13 of the 2026-08-15 optimization pass.
// Returns `404 Not Found` if the tool is not registered.
pub async fn unregister_tool(
    State(state): State<Arc<AppState>>,
    AppPath(name): AppPath<String>,
) -> Result<Json<ToolInfo>, AppError> {
    validate_resource_name(&name)?;
    let reg = state.tool_registry.write().await;
    let entries = reg.list(None).await.unwrap_or_default();
    let target =
        entries
            .into_iter()
            .find(|e| e.name() == name)
            .ok_or_else(|| {
                AppError::from(Error::not_found(format!("tool '{name}'")))
            })?;
    let description = target.description().to_string();
    reg.delete(&name).await.map_err(|e| {
        Error::internal(format!("failed to unregister tool: {e}"))
    })?;
    Ok(Json(ToolInfo { name, description }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hermetic `AppState` for route tests (same shape as the
    /// helper in `routes::health`).
    async fn test_state() -> (Arc<AppState>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp workspace");
        let sessions = synthia::session::manager::SessionRegistry::new(
            dir.path().join("sessions"),
        );
        let state =
            AppState::for_test(sessions, dir.path().to_path_buf()).await;
        (Arc::new(state), dir)
    }

    fn schema(argument: &str) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": { argument: {"type": "string"} },
            "required": [argument],
        })
    }

    /// The tools endpoint reports the *model-facing* list — the same
    /// projection the agent loop uses. A `Deferred` tool is advertised
    /// with the permissive placeholder rather than its real schema,
    /// and an `is_hidden` tool is absent entirely.
    #[tokio::test]
    async fn collect_tool_defs_projects_exposure_and_privacy() {
        let (state, _dir) = test_state().await;
        {
            let registry = state.tool_registry.read().await;
            registry.register_entry(synthia::tool::ToolEntry::dynamic(
                "alpha".to_string(),
                "a direct tool".to_string(),
                schema("a"),
            ));
            registry.register_entry(
                synthia::tool::ToolEntry::dynamic(
                    "beta".to_string(),
                    "a deferred tool".to_string(),
                    schema("b"),
                )
                .with_exposure(synthia::tool::ToolExposure::Deferred),
            );
            registry.register_entry(
                synthia::tool::ToolEntry::dynamic(
                    "secret".to_string(),
                    "a private tool".to_string(),
                    schema("s"),
                )
                .with_is_hidden(true),
            );
        }

        let defs = collect_tool_defs(&state).await;
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"alpha"), "got {names:?}");
        assert!(names.contains(&"beta"), "got {names:?}");
        assert!(
            !names.contains(&"secret"),
            "an is_hidden tool must not be advertised; got {names:?}"
        );
        let alpha = defs
            .iter()
            .find(|d| d.name == "alpha")
            .expect("alpha is advertised");
        assert_eq!(
            alpha.input_schema,
            schema("a"),
            "a Direct tool keeps its real schema"
        );
        let beta = defs
            .iter()
            .find(|d| d.name == "beta")
            .expect("beta is advertised");
        assert_eq!(
            beta.input_schema,
            serde_json::json!({
                "type": "object",
                "additionalProperties": true,
            }),
            "a Deferred tool is advertised with the placeholder schema"
        );
    }

    #[test]
    fn tool_info_serializes_name_and_description() {
        let info = ToolInfo {
            name: "bash".to_string(),
            description: "Execute shell commands".to_string(),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["name"], "bash");
        assert_eq!(json["description"], "Execute shell commands");
    }

    #[test]
    fn tool_detail_serializes_with_input_schema() {
        let detail = ToolDetail {
            name: "bash".to_string(),
            description: "Execute shell commands".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"cmd": {"type": "string"}},
                "required": ["cmd"],
            }),
            provenance: "core".to_string(),
            output_definition:
                synthia::tool::ToolOutputDefinition::passthrough("bash")
                    .with_kind(synthia::tool::RenderKind::Shell),
        };
        let json = serde_json::to_value(&detail).unwrap();
        assert_eq!(json["name"], "bash");
        assert_eq!(json["input_schema"]["type"], "object");
        assert_eq!(json["input_schema"]["required"][0], "cmd");
        assert_eq!(json["provenance"], "core");
        // R17: the render contract rides the detail envelope.
        assert_eq!(json["output_definition"]["kind"], "shell");
        assert_eq!(json["output_definition"]["name"], "bash");
    }
}
