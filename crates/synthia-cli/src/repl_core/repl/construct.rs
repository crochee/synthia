//! Constructors and entry points.
//!
//! Owns [`Repl::new`], [`ReplContext::new`], the
//! `load_skill_summaries` helper, the free [`run`] /
//! [`run_with_context`] entry points, and the
//! [`current_user_id`] identity helper.
//!
//! §1 invariant: every `SessionStore` call in this
//! crate is scoped to the user id returned by
//! [`current_user_id`]; the `user_id` MUST be non-empty
//! and is fatal at the REPL boundary on identity load
//! failure.

use std::{path::PathBuf, sync::Arc};

use synthia_command::CommandRegistry;
use synthia_core::generate_session_id;
use synthia_memory::{episodic::EpisodicMemory, hot::HotMemory};
use synthia_provider::config::WorkspaceConfig;

use super::types::{Repl, ReplContext};
use crate::{Identity, workspace::WorkspaceInfo};

/// Load the CLI's stable per-machine identity and return its
/// `user_id`. All `SessionStore` operations in this module are
/// scoped to the returned id; the §1 invariant guarantees that no
/// session can be created, listed, or deleted outside this
/// namespace. Errors are surfaced verbatim to the REPL via
/// `IdentityError`; the caller is expected to render them.
pub(super) fn current_user_id() -> Result<String, crate::identity::IdentityError>
{
    Identity::load_or_create().map(|id| id.user_id().to_string())
}

impl Repl {
    /// Create a new REPL instance with the given workspace root.
    pub fn new(workspace_root: PathBuf) -> Self {
        let registry = CommandRegistry::new();
        registry.register_builtins();
        registry.load_user_commands(&workspace_root);

        Self {
            command_registry: registry,
            workspace_root,
            state: parking_lot::RwLock::new(super::types::SessionState::new()),
        }
    }
}

impl ReplContext {
    /// Create a new REPL context from the workspace root.
    pub async fn new(workspace_root: PathBuf) -> Self {
        let workspace_config =
            WorkspaceConfig::load_from_dir(&workspace_root).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Failed to load workspace config, using env fallback");
                WorkspaceConfig::from_env()
            });

        let current_model = workspace_config.default_model.clone();
        let current_provider_name = workspace_config.default_provider.clone();

        let provider = workspace_config
            .create_default_provider()
            .map(Arc::from)
            .ok();

        if provider.is_none() {
            tracing::warn!(
                "No LLM provider available. Configure .agents/config.toml or set environment variables."
            );
        }

        let session_id = generate_session_id();

        let agents_dir = workspace_root.join(".agents");
        let hot_memory = Arc::new(HotMemory::new(agents_dir.clone()));
        if let Err(e) = hot_memory.load_from_disk().await {
            tracing::warn!(error = %e, "Failed to load hot memory from disk");
        }

        let episodic_memory = EpisodicMemory::new(agents_dir.clone())
            .await
            .map(Arc::new)
            .ok();

        let skill_summaries = Self::load_skill_summaries(&agents_dir).await;

        Self {
            provider,
            workspace_root,
            workspace_config,
            current_model,
            current_provider_name,
            session_id,
            hot_memory: Some(hot_memory),
            episodic_memory,
            skill_summaries,
        }
    }

    pub(super) async fn load_skill_summaries(
        agents_dir: &std::path::Path,
    ) -> Option<String> {
        let skills_dir = agents_dir.join("skills");
        if !skills_dir.exists() {
            return None;
        }

        let entries = std::fs::read_dir(&skills_dir).ok()?;
        let mut summaries = Vec::new();

        for entry in entries.flatten() {
            let skill_md = entry.path().join("SKILL.md");
            if skill_md.exists()
                && let Ok(meta) =
                    synthia_skill::loader::SkillLoader::parse_frontmatter(
                        &skill_md,
                    )
            {
                summaries.push(format!(
                    "- [{}] {} (v{})",
                    meta.name,
                    meta.description.chars().take(60).collect::<String>(),
                    meta.version.as_deref().unwrap_or("unknown"),
                ));
            }
        }

        if summaries.is_empty() {
            None
        } else {
            Some(summaries.join("\n"))
        }
    }
}

/// Public entry point for the REPL - matches the previous interface used by main.rs.
pub async fn run(workspace: &WorkspaceInfo) -> anyhow::Result<()> {
    let mut repl = Repl::new(workspace.root.clone());
    repl.run(workspace).await
}

/// Run the REPL with an existing context.
pub async fn run_with_context(
    workspace: &WorkspaceInfo,
    ctx: ReplContext,
) -> anyhow::Result<()> {
    let mut repl = Repl::new(workspace.root.clone());
    let mut ctx_mut = ctx;
    repl.run_with_context(workspace, &mut ctx_mut).await
}
