use std::sync::Arc;

use axum::{Json, extract::State};
use parking_lot::RwLock;
use serde::Serialize;
use synthia_core::{
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
}

/// Cached `(registry_version, defs)` for `collect_tool_defs`.
///
/// `ToolDefinition::input_schema` is built from `parameters()`,
/// which (for built-in tools) calls `schemars::schema_for!` and
/// `serde_json::to_value` on every invocation — that's tens of
/// microseconds per tool × N tools per list request. Built-in
/// tool schemas are immutable (computed from a `#[derive(Schemars)]`
/// struct); dynamic tool schemas are stored on the
/// registration and also immutable. Caching the full
/// `Vec<ToolDefinition>` keyed on the registry's monotonic
/// version (see `ToolRegistry::version`) avoids paying that cost
/// on every request. The cache busts transparently on the next
/// `register_tool` / `unregister_tool` call.
static TOOL_DEFS_CACHE: RwLock<
    Option<(u64, Arc<Vec<synthia_provider::ToolDefinition>>)>,
> = RwLock::new(None);

/// Collect tool definitions from the registry, keyed by the
/// registry's monotonic version. Hit: Arc clone (8-byte
/// refcount) + full Vec deep clone for the caller (the schema
/// `serde_json::Value` clones are unavoidable but only happen on
/// the consumer side, not the registry side). Miss: acquire
/// the registry read lock, walk the entries, build the full
/// `ToolDefinition`s, store under the current version.
async fn collect_tool_defs(
    state: &Arc<AppState>,
) -> Vec<synthia_provider::ToolDefinition> {
    let version = state.tool_registry.read().await.version();
    {
        let guard = TOOL_DEFS_CACHE.read();
        if let Some((v, cached)) = guard.as_ref()
            && *v == version
        {
            return (**cached).clone();
        }
    }
    let tool_reg = state.tool_registry.read().await;
    let defs: Vec<synthia_provider::ToolDefinition> = tool_reg
        .list(None)
        .await
        .map(|entries| {
            entries
                .iter()
                .map(|e| synthia_provider::ToolDefinition {
                    name: e.name().to_string(),
                    description: e.description().to_string(),
                    input_schema: e.tool_instance().parameters(),
                    cache_control: None,
                })
                .collect()
        })
        .unwrap_or_default();
    drop(tool_reg);
    let arc = Arc::new(defs);
    *TOOL_DEFS_CACHE.write() = Some((version, arc.clone()));
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
    let tool = synthia_tool::ToolEntry::dynamic(
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
            synthia_tool::registry::ToolProvenance::Core => "core",
            synthia_tool::registry::ToolProvenance::Dynamic => "dynamic",
        })
        .unwrap_or("dynamic")
        .to_string();
    let description = entry.description().to_string();
    let input_schema = entry.tool_instance().parameters();
    Ok(Json(ToolDetail {
        name: entry.name().to_string(),
        description,
        input_schema,
        provenance,
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
        };
        let json = serde_json::to_value(&detail).unwrap();
        assert_eq!(json["name"], "bash");
        assert_eq!(json["input_schema"]["type"], "object");
        assert_eq!(json["input_schema"]["required"][0], "cmd");
        assert_eq!(json["provenance"], "core");
    }
}
