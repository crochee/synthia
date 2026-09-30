//! Pluggable skill provider registry — dsh `SkillRegistry` parity.
//!
//! ## Motivation
//!
//! Synthia discovers skills from two filesystem roots (project +
//! user) via the [`crate::discovery::discover_skills`] function. A lib
//! consumer who wants to load skills from a **remote registry**, a
//! **database**, or a **programmatic in-memory list** must today
//! either fork the discovery module or shadow the value type.
//!
//! R9-2 turns discovery into one of many `SkillProvider` trait impls.
//! The lib consumer does:
//!
//! ```ignore
//! use synthia_skill::{SkillRegistry, Skill, SkillProvider, SkillCandidate};
//!
//! let mut reg = SkillRegistry::new();
//! reg.register(FileSkillProvider::discover(workspace));
//! reg.register(RemoteSkillProvider::new(remote_client));
//! reg.register(InMemorySkillProvider::new(vec![my_skill]));
//! let skills = reg.collect();
//! ```
//!
//! The layered precedence matches dsh's `BUNDLED_SKILL_RANK` /
//! `RUNTIME_RANK` ordering: bundled skills outrank user skills
//! outrank project skills outrank runtime-only skills. Two skills
//! with the same name resolve to the higher-ranked provider's copy;
//! lower-ranked copies are silently dropped (matches the opencode
//! "project wins over user" rule).
//!
//! ## Layering
//!
//! ```text
//! SkillRegistry          the public façade
//!   ├─ SkillProvider trait impls (file, remote, in-memory, …)
//!   └─ candidates         the deduplicated `Vec<Skill>` returned by `collect()`
//! ```
//!
//! ## Reference
//!
//! Adopted from dsh `packages/skill/skill/src/index.ts`:
//! `SkillProvider` interface (lines 248-268) + `SkillLayer` +
//! `SkillRegistry` class (line 661). Synthia's port renames the
//! `BUNDLED_SKILL_RANK` constant to [`SkillRank`] and exposes it
//! through the [`SkillRegistry::register`] builder so lib consumers can register
//! their own providers with custom precedence.

use std::fmt;

use crate::skill::Skill;

/// Precedence rank a provider contributes when its candidates are
/// merged into the registry.
///
/// Higher numbers win — when two providers both list a skill with
/// the same `name`, the higher-ranked provider's copy survives and
/// the lower-ranked copy is silently dropped. Ranks are an
/// `i32` rather than an enum so lib consumers can plug in their
/// own providers without modifying the source.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SkillRank(pub i32);

impl SkillRank {
    /// Bundled (shipped-with-synthia) skills outrank everything
    /// else — matches dsh `BUNDLED_SKILL_RANK = 600`.
    pub const BUNDLED: SkillRank = SkillRank(600);
    /// Workspace project skills (`.agents/skills/`). Lower than
    /// user-home so a user can shadow a project skill by naming a
    /// skill with the same id in their home directory. Matches
    /// the dsh "user > project" rule.
    pub const PROJECT: SkillRank = SkillRank(400);
    /// Runtime-registered skills (in-memory or programmatic
    /// providers registered via [`SkillRegistry::register`]).
    /// Lowest precedence — runtime skills never shadow a
    /// filesystem-resident skill with the same name.
    pub const RUNTIME: SkillRank = SkillRank(250);
    /// User-home skills (Anthropic `~/.claude/skills`,
    /// OpenCode `~/.agents/skills`).
    pub const USER: SkillRank = SkillRank(500);

    /// Build a custom rank from any `i32`. Used by lib consumers
    /// that want a precedence between two built-in ranks (e.g.
    /// `SkillRank::custom(450)` to outrank project but lose to
    /// user).
    pub const fn custom(value: i32) -> Self {
        SkillRank(value)
    }
}

impl Default for SkillRank {
    fn default() -> Self {
        Self::USER
    }
}

impl fmt::Display for SkillRank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SkillRank({})", self.0)
    }
}

/// Optional metadata a provider can attach to its contribution.
///
/// Providers that don't need any extra fields can use
/// [`SkillProviderInfo::default()`].
#[derive(Clone, Debug, Default)]
pub struct SkillProviderInfo {
    /// Stable name (used in log breadcrumbs and the registry
    /// facade's `providers()` listing).
    pub name: String,
    /// Free-form description (suitable for diagnostics; not
    /// surfaced to the model).
    pub description: String,
}

impl SkillProviderInfo {
    /// Build a `SkillProviderInfo` from a name and description.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
        }
    }
}

/// The contract every skill source implements.
///
/// One provider contributes zero or more [`Skill`] values. Providers
/// may be stateful (file-walkers cache directory mtimes, remote
/// providers hold a connection pool, etc.) — the trait is
/// `Send + Sync` so the registry can drive providers from any
/// thread.
///
/// ## Provider vs Source
///
/// A "source" returns raw `Skill` values; a provider contributes a
/// *set* of candidates tagged with a [`SkillRank`] and optional
/// [`SkillProviderInfo`]. The registry merges across all providers
/// and dedups by name (higher rank wins).
#[async_trait::async_trait]
pub trait SkillProvider: Send + Sync {
    /// Stable identifier for diagnostics.
    fn info(&self) -> SkillProviderInfo;

    /// Precedence rank for this provider's candidates.
    fn rank(&self) -> SkillRank;

    /// Collect every skill this provider contributes.
    ///
    /// Implementations may use any async backend (filesystem walk,
    /// remote fetch, in-memory vec). Errors are surfaced via
    /// [`SkillProviderError`] so a single failing provider doesn't
    /// poison the whole registry — [`SkillRegistry::collect`]
    /// converts a provider failure into a logged warning and an
    /// empty candidate list.
    async fn collect(&self) -> Result<Vec<Skill>, SkillProviderError>;
}

/// Errors a provider can surface during `collect`.
#[derive(Debug, thiserror::Error)]
pub enum SkillProviderError {
    /// Filesystem-level failure (permission denied, ENOENT, etc.).
    #[error("filesystem error walking {path}: {message}")]
    Filesystem {
        /// Path the provider was walking when the error occurred.
        path: String,
        /// Underlying I/O error message.
        message: String,
    },
    /// Network-level failure (timeout, refused connection, …).
    #[error("network error fetching {url}: {message}")]
    Network {
        /// URL the provider was fetching when the error occurred.
        url: String,
        /// Underlying error message.
        message: String,
    },
    /// Provider-specific validation failure (a candidate failed the
    /// skill loader's frontmatter check, a remote record was
    /// malformed, …).
    #[error("invalid skill {name}: {message}")]
    Invalid {
        /// Name of the offending candidate, when known.
        name: String,
        /// Underlying error message.
        message: String,
    },
    /// Catch-all for provider implementations that want to surface
    /// a custom error.
    #[error("{0}")]
    Other(String),
}

/// A boxed [`SkillProvider`] for storage in the registry.
pub type BoxedSkillProvider = Box<dyn SkillProvider>;

/// A `SkillProvider` that contributes a fixed in-memory list.
///
/// Useful for tests, tutorials, and lib consumers that want to
/// preload a curated skill set without walking the filesystem.
pub struct InMemorySkillProvider {
    info: SkillProviderInfo,
    rank: SkillRank,
    skills: Vec<Skill>,
}

impl InMemorySkillProvider {
    /// Build an in-memory provider with the default
    /// [`SkillRank::USER`] precedence.
    pub fn new(skills: Vec<Skill>) -> Self {
        Self {
            info: SkillProviderInfo::new("in-memory", "in-memory skill list"),
            rank: SkillRank::USER,
            skills,
        }
    }

    /// Build an in-memory provider with explicit rank + info.
    pub fn with_rank(
        rank: SkillRank,
        info: SkillProviderInfo,
        skills: Vec<Skill>,
    ) -> Self {
        Self { info, rank, skills }
    }

    /// Replace the in-memory skill list (used by tests that want
    /// to mutate the contribution between `collect()` calls).
    pub fn set_skills(&mut self, skills: Vec<Skill>) {
        self.skills = skills;
    }
}

#[async_trait::async_trait]
impl SkillProvider for InMemorySkillProvider {
    fn info(&self) -> SkillProviderInfo {
        self.info.clone()
    }

    fn rank(&self) -> SkillRank {
        self.rank
    }

    async fn collect(&self) -> Result<Vec<Skill>, SkillProviderError> {
        Ok(self.skills.clone())
    }
}

/// Result of one [`SkillRegistry::collect`] call — the deduplicated
/// `Vec<Skill>` plus the per-provider statistics for diagnostics.
#[derive(Clone, Debug, Default)]
pub struct SkillCollectReport {
    /// The deduplicated skill list (the registry's primary output).
    pub skills: Vec<Skill>,
    /// Per-provider statistics: provider name → number of candidates
    /// it contributed **before** dedup.
    pub per_provider: Vec<PerProviderReport>,
    /// Number of providers that failed during `collect()` (the
    /// registry continues collecting from other providers when one
    /// fails — see [`SkillRegistry::collect`]).
    pub failed_providers: Vec<(String, String)>,
}

/// One provider's contribution to a [`SkillCollectReport`].
#[derive(Clone, Debug)]
pub struct PerProviderReport {
    /// Provider's [`SkillProvider::info`].`name`.
    pub provider_name: String,
    /// Provider's [`SkillProvider::rank`].
    pub rank: SkillRank,
    /// Number of candidates contributed (before dedup).
    pub candidate_count: usize,
    /// Number of those candidates that survived into the final
    /// `skills` list (others were shadowed by a higher-ranked
    /// provider with the same skill name).
    pub survived_count: usize,
}

/// The typed registry — the public façade that owns a list of
/// `SkillProvider` impls and produces a deduplicated `Vec<Skill>`
/// when asked.
///
/// ## Construction
///
/// ```ignore
/// use synthia_skill::{SkillRegistry, FileSkillProvider, InMemorySkillProvider};
///
/// let mut reg = SkillRegistry::new();
/// reg.register(FileSkillProvider::discover(workspace));
/// reg.register(InMemorySkillProvider::new(vec![my_skill]));
/// let skills = reg.collect().await.skills;
/// ```
///
/// `SkillRegistry` does **not** itself perform any filesystem walk;
/// every discovery path is a `SkillProvider` impl the lib consumer
/// can opt into (or replace).
pub struct SkillRegistry {
    providers: Vec<BoxedSkillProvider>,
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRegistry {
    /// Empty registry — callers `register` providers individually.
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Register a provider. Providers are stored in insertion order
    /// (the registry dedups across them by name + rank regardless
    /// of insertion order).
    ///
    /// The provider's [`SkillRank`] decides precedence against
    /// every other provider's contribution.
    pub fn register<P: SkillProvider + 'static>(
        &mut self,
        provider: P,
    ) -> &mut Self {
        self.providers.push(Box::new(provider));
        self
    }

    /// Register a pre-built `Box<dyn SkillProvider>` (useful when
    /// the provider type is dynamic).
    pub fn register_boxed(
        &mut self,
        provider: BoxedSkillProvider,
    ) -> &mut Self {
        self.providers.push(provider);
        self
    }

    /// Number of registered providers.
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// True when no provider is registered.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// Names of every registered provider (diagnostics).
    pub fn provider_names(&self) -> Vec<String> {
        self.providers.iter().map(|p| p.info().name).collect()
    }

    /// Drive every provider, collect their contributions, dedup by
    /// name (higher-ranked provider wins), and return the
    /// deduplicated `Vec<Skill>` plus per-provider stats.
    ///
    /// A provider that errors out during `collect()` is logged
    /// into [`SkillCollectReport::failed_providers`] and the
    /// remaining providers continue. This matches the opencode
    /// leniency: a malformed third-party skill never poisons the
    /// registry.
    pub async fn collect(&self) -> SkillCollectReport {
        let mut report = SkillCollectReport::default();
        // Sort providers by descending rank so when we walk the
        // map (insertion order), higher-ranked contributions
        // overwrite lower-ranked ones.
        let mut indexed: Vec<&BoxedSkillProvider> =
            self.providers.iter().collect();
        indexed.sort_by_key(|p| std::cmp::Reverse(p.rank()));

        // Track the rank that "owns" each surviving skill name.
        let mut owner: std::collections::HashMap<String, SkillRank> =
            std::collections::HashMap::new();
        // Track the per-provider candidate count (regardless of
        // whether the candidate survived dedup).
        let mut candidate_count: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for provider in &indexed {
            let info = provider.info();
            let name = info.name.clone();
            let rank = provider.rank();
            match provider.collect().await {
                Ok(skills) => {
                    candidate_count.insert(name.clone(), skills.len());
                    for skill in skills {
                        match owner.get(&skill.name) {
                            None => {
                                owner.insert(skill.name.clone(), rank);
                                report.skills.push(skill);
                            }
                            Some(existing_rank) if *existing_rank < rank => {
                                // The existing lower-ranked copy is
                                // shadowed — remove it and insert
                                // the higher-ranked one.
                                if let Some(pos) = report
                                    .skills
                                    .iter()
                                    .position(|s| s.name == skill.name)
                                {
                                    report.skills.remove(pos);
                                }
                                owner.insert(skill.name.clone(), rank);
                                report.skills.push(skill);
                            }
                            Some(_) => {
                                // Same or higher-ranked copy already
                                // won; silently drop this candidate.
                            }
                        }
                    }
                }
                Err(err) => {
                    report
                        .failed_providers
                        .push((name.clone(), err.to_string()));
                }
            }
        }

        // Build per-provider reports (after the dedup pass so the
        // survived count is accurate).
        for provider in &indexed {
            let info = provider.info();
            let name = info.name;
            let rank = provider.rank();
            let candidates = candidate_count.get(&name).copied().unwrap_or(0);
            let survived = report
                .skills
                .iter()
                .filter(|s| {
                    owner.get(&s.name).copied() == Some(rank)
                        && candidate_count.get(&name).copied().unwrap_or(0) > 0
                })
                .count();
            report.per_provider.push(PerProviderReport {
                provider_name: name,
                rank,
                candidate_count: candidates,
                survived_count: survived,
            });
        }

        report
    }
}

/// Helper that wraps [`crate::discovery::discover_skills`] as a
/// `SkillProvider`. Lib consumers don't need to construct one by
/// hand — `SkillRegistry::discover_files(workspace_root)` does the
/// work for them.
pub struct FileSkillProvider {
    workspace_root: std::path::PathBuf,
    rank: SkillRank,
}

impl FileSkillProvider {
    /// Build a provider that walks the project + user skill roots
    /// for `workspace_root`.
    pub fn discover(workspace_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            rank: SkillRank::PROJECT,
        }
    }

    /// Override the default [`SkillRank::PROJECT`] (e.g. for
    /// tests that want a "lowest-priority" provider).
    pub fn with_rank(mut self, rank: SkillRank) -> Self {
        self.rank = rank;
        self
    }
}

#[async_trait::async_trait]
impl SkillProvider for FileSkillProvider {
    fn info(&self) -> SkillProviderInfo {
        SkillProviderInfo::new(
            "filesystem",
            format!(
                "walks {} + $HOME/.claude/skills + $HOME/.agents/skills",
                self.workspace_root.display()
            ),
        )
    }

    fn rank(&self) -> SkillRank {
        self.rank
    }

    async fn collect(&self) -> Result<Vec<Skill>, SkillProviderError> {
        Ok(crate::discovery::discover_skills(&self.workspace_root))
    }
}

impl SkillRegistry {
    /// Convenience constructor that registers a
    /// [`FileSkillProvider`] for `workspace_root` (matches the
    /// default project-precedence file walk). Lib consumers who
    /// need only the legacy `discover_skills` behaviour can use
    /// this in place of the `discover_skills` function.
    pub fn discover_files(
        workspace_root: impl Into<std::path::PathBuf>,
    ) -> Self {
        let mut reg = Self::new();
        reg.register(FileSkillProvider::discover(workspace_root));
        reg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_skill(name: &str) -> Skill {
        Skill {
            name: name.to_string(),
            description: Some(format!("desc for {name}")),
            location: std::path::PathBuf::from(format!("/tmp/{name}/SKILL.md")),
            content: format!("# {name}\nbody content"),
        }
    }

    #[tokio::test]
    async fn empty_registry_returns_empty_report() {
        let reg = SkillRegistry::new();
        let report = reg.collect().await;
        assert!(report.skills.is_empty());
        assert!(report.per_provider.is_empty());
        assert!(report.failed_providers.is_empty());
    }

    #[tokio::test]
    async fn in_memory_provider_contributes_its_skills() {
        let skills = vec![make_skill("alpha"), make_skill("beta")];
        let mut reg = SkillRegistry::new();
        reg.register(InMemorySkillProvider::new(skills.clone()));
        let report = reg.collect().await;
        assert_eq!(report.skills.len(), 2);
        assert_eq!(report.failed_providers.len(), 0);
    }

    #[tokio::test]
    async fn higher_ranked_provider_shadows_lower_for_same_name() {
        let mut reg = SkillRegistry::new();
        reg.register(InMemorySkillProvider::with_rank(
            SkillRank::PROJECT,
            SkillProviderInfo::new("project", "project skill"),
            vec![make_skill("shared")],
        ));
        reg.register(InMemorySkillProvider::with_rank(
            SkillRank::USER,
            SkillProviderInfo::new("user", "user skill"),
            vec![make_skill("shared")],
        ));
        let report = reg.collect().await;
        // Only the user-skill survives (higher rank wins).
        assert_eq!(report.skills.len(), 1);
        // The location of the surviving skill matches the user
        // provider (rank 500), not the project provider (rank 400).
        assert!(report.skills[0].location.starts_with("/tmp/shared"));
    }

    #[tokio::test]
    async fn providers_with_distinct_names_are_all_kept() {
        let mut reg = SkillRegistry::new();
        reg.register(InMemorySkillProvider::new(vec![make_skill("alpha")]));
        reg.register(InMemorySkillProvider::new(vec![make_skill("beta")]));
        let report = reg.collect().await;
        assert_eq!(report.skills.len(), 2);
    }

    #[tokio::test]
    async fn failing_provider_is_logged_and_does_not_poison_registry() {
        struct AlwaysFails;
        #[async_trait::async_trait]
        impl SkillProvider for AlwaysFails {
            fn info(&self) -> SkillProviderInfo {
                SkillProviderInfo::new("fails", "always fails")
            }

            fn rank(&self) -> SkillRank {
                SkillRank::USER
            }

            async fn collect(&self) -> Result<Vec<Skill>, SkillProviderError> {
                Err(SkillProviderError::Other("simulated failure".to_string()))
            }
        }
        let mut reg = SkillRegistry::new();
        reg.register(InMemorySkillProvider::new(vec![make_skill("alpha")]));
        reg.register(AlwaysFails);
        let report = reg.collect().await;
        // The failing provider must be logged but the other
        // provider's skills still appear.
        assert_eq!(report.skills.len(), 1);
        assert_eq!(report.failed_providers.len(), 1);
        assert_eq!(report.failed_providers[0].0, "fails");
    }

    #[tokio::test]
    async fn provider_names_lists_in_insertion_order() {
        let mut reg = SkillRegistry::new();
        reg.register(InMemorySkillProvider::with_rank(
            SkillRank::USER,
            SkillProviderInfo::new("a", "first"),
            vec![],
        ));
        reg.register(InMemorySkillProvider::with_rank(
            SkillRank::USER,
            SkillProviderInfo::new("b", "second"),
            vec![],
        ));
        assert_eq!(reg.provider_names(), vec!["a", "b"]);
        assert_eq!(reg.len(), 2);
        assert!(!reg.is_empty());
    }

    #[test]
    fn skill_rank_ordering_is_total() {
        // Sanity check: the four built-in ranks compare correctly.
        assert!(SkillRank::BUNDLED > SkillRank::USER);
        assert!(SkillRank::USER > SkillRank::PROJECT);
        assert!(SkillRank::PROJECT > SkillRank::RUNTIME);
    }

    /// The file provider must delegate to crate::discovery so
    /// the legacy behavior stays available.
    ///
    /// `collect()` merges user skills from `$HOME`, so the
    /// whole body — including driving the never-yielding
    /// future — runs inside `with_scoped_home` to pin `$HOME`
    /// to an empty tempdir. Without the scope this test read
    /// the developer's real skill set and flaked on machines
    /// with installed skills.
    #[test]
    fn file_provider_uses_discovery_under_the_hood() {
        crate::test_util::with_scoped_home(|_home| {
            let tmp = tempfile::tempdir().expect("tempdir");
            let skill_dir = tmp.path().join(".agents/skills/example");
            std::fs::create_dir_all(&skill_dir).expect("mkdir");
            std::fs::write(
                skill_dir.join("SKILL.md"),
                "---\nname: example\ndescription: example skill\n---\n# example\nbody",
            )
            .expect("write");
            let mut reg = SkillRegistry::new();
            reg.register(FileSkillProvider::discover(tmp.path()));
            let report = futures::executor::block_on(reg.collect());
            assert_eq!(report.skills.len(), 1);
            assert_eq!(report.skills[0].name, "example");
        });
    }

    #[test]
    fn skill_provider_error_display_is_human_readable() {
        let err = SkillProviderError::Filesystem {
            path: "/tmp/skills".to_string(),
            message: "permission denied".to_string(),
        };
        let formatted = format!("{err}");
        assert!(formatted.contains("/tmp/skills"));
        assert!(formatted.contains("permission denied"));
    }

    #[test]
    fn skill_rank_default_is_user() {
        assert_eq!(SkillRank::default(), SkillRank::USER);
    }
}
