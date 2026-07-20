use synthia_permission::Permission;

/// Return `true` if `a` is more restrictive than `b`.
///
/// Permission ordering (most -> least restrictive):
/// Block > Deny > RequireExplicit > RequireConfirm > AutoApprove
pub fn permission_is_more_restrictive(a: &Permission, b: &Permission) -> bool {
    fn restrictiveness(p: &Permission) -> u8 {
        match p {
            Permission::Block => 4,
            Permission::Deny { .. } => 3,
            Permission::RequireExplicit => 2,
            Permission::RequireConfirm => 1,
            Permission::AutoApprove => 0,
        }
    }
    restrictiveness(a) > restrictiveness(b)
}

/// Apply the provenance floor to a permission level.
///
/// Each provenance category sets a minimum permission level. If the policy
/// grants a more permissive level, the provenance floor overrides it. If the
/// policy is already more restrictive, the policy wins.
///
/// | Provenance  | Floor             |
/// |-------------|-------------------|
/// | `Builtin`   | `AutoApprove`     |
/// | `Plugin`    | `RequireConfirm`  |
/// | `Ephemeral` | `RequireExplicit` |
pub fn apply_provenance_floor(
    provenance: &synthia_tool_materialization::ToolProvenance,
    permission: Permission,
) -> Permission {
    use synthia_tool_materialization::ToolProvenance;
    let floor = match provenance {
        ToolProvenance::Builtin => Permission::AutoApprove,
        ToolProvenance::Plugin { .. } => Permission::RequireConfirm,
        ToolProvenance::Ephemeral { .. } => Permission::RequireExplicit,
    };
    if permission_is_more_restrictive(&permission, &floor) {
        permission
    } else {
        floor
    }
}

/// Resolves a tool name to its [`ToolProvenance`](synthia_tool_materialization::ToolProvenance).
///
/// Implementations typically consult the tool registry or
/// materialization store to determine where a tool originated.
pub trait ToolProvenanceResolver: Send + Sync {
    fn resolve_provenance(
        &self,
        tool_name: &str,
    ) -> Option<synthia_tool_materialization::ToolProvenance>;
}

/// Map a tool name to its primary capability string used by the
/// [`CapabilityBroker`](synthia_core::tool::capability::CapabilityBroker). Returns `None` for tools that do not declare
/// a capability (i.e. pure/safe tools that need no gating).
pub(crate) fn capability_for_tool_name(name: &str) -> Option<&'static str> {
    match name {
        "bash" => Some("command_invoke"),
        "read" | "glob" | "grep" => Some("memory_read"),
        "write" | "multi_edit" | "apply_patch" => Some("memory_write"),
        _ => None,
    }
}

pub(crate) fn extract_file_path(
    tool_name: &str,
    arguments: &serde_json::Value,
    workspace_root: &std::path::Path,
) -> Option<std::path::PathBuf> {
    match tool_name {
        "write" | "multi_edit" => {}
        _ => return None,
    }
    let path_str = arguments.get("path").and_then(|v| v.as_str())?;
    let p = std::path::PathBuf::from(path_str);
    Some(if p.is_absolute() {
        p
    } else {
        workspace_root.join(p)
    })
}

pub(crate) fn is_read_tool(tool_name: &str) -> bool {
    tool_name == "read"
}

pub(crate) fn is_write_tool(tool_name: &str) -> bool {
    matches!(tool_name, "write" | "multi_edit")
}
