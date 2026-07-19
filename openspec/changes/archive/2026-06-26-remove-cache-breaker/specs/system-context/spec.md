## ADDED Requirements

### Requirement: SystemContext SHALL provide git environment information

`SystemContext` SHALL expose the current git branch name and a summarized git status. The git branch accessor SHALL return `Option<&str>`; the git status accessor SHALL return `Option<&str>`. Both accessors SHALL return `None` when git information is unavailable (e.g., not a git repository).

#### Scenario: Git branch available

- **WHEN** `get_system_context()` is called inside a git repository with a valid HEAD
- **THEN** the returned `SystemContext.git_branch()` SHALL return `Some(&str)` containing the abbreviated branch name

#### Scenario: Not a git repository

- **WHEN** `get_system_context()` is called outside any git repository
- **THEN** the returned `SystemContext.git_branch()` SHALL return `None` and `git_status()` SHALL return `None`

#### Scenario: Clean working tree

- **WHEN** the git working tree has no uncommitted changes
- **THEN** `SystemContext.git_status()` SHALL return `Some("clean")`

#### Scenario: Dirty working tree

- **WHEN** the git working tree has uncommitted changes
- **THEN** `SystemContext.git_status()` SHALL return `Some(<summary>)` where `<summary>` joins the first 3 porcelain status lines with `"; "`

---

### Requirement: SystemContext SHALL NOT contain cache-breaking fields

`SystemContext` MUST NOT contain any field whose purpose is to perturb LLM API cache prefixes (e.g., a `cache_breaker` random string). Cache namespace isolation SHALL be handled by `prompt_cache_key` (user_id namespaced); cache policy application SHALL be handled by `applyCachePolicy`. `SystemContext::new()` SHALL take no arguments.

#### Scenario: Constructing SystemContext

- **WHEN** `SystemContext::new()` is called
- **THEN** it SHALL return a `SystemContext` with `git_branch: None`, `git_status: None`, and `beta_headers: Vec::new()`
- **AND** the result SHALL NOT contain any random or cache-breaker field

#### Scenario: No random perturbation across calls

- **WHEN** `SystemContext::new()` is called twice in succession
- **THEN** both results SHALL be structurally identical (no per-call randomness injected by construction)

---

### Requirement: SystemContext SHALL be cached with a 5-minute TTL

`get_system_context()` SHALL cache the `SystemContext` for 300 seconds. Calls within the TTL SHALL return the cached value without re-invoking git subprocesses. `clear_system_context_cache()` SHALL invalidate the cache so the next `get_system_context()` call re-fetches fresh git data.

#### Scenario: Cache hit within TTL

- **WHEN** `get_system_context()` is called twice within 300 seconds
- **THEN** the second call SHALL return a clone of the cached `SystemContext` without spawning git subprocesses

#### Scenario: Cache cleared

- **WHEN** `clear_system_context_cache()` is called followed by `get_system_context()`
- **THEN** the subsequent `get_system_context()` SHALL re-run git subprocesses and populate a fresh cache entry
