//! Shared test fixtures for `state/` tests.
//!
//! Kept narrow on purpose — only what more than one
//! submodule needs:
//!
//! - [`StubAgent`] / [`stub_entry`] — a minimal
//!   `RegistryItem`+`Agent` the registry tests can register
//!   without a real model provider behind it.
//! - [`ScopedHome`] — a `$HOME` rewriter that pins the
//!   process-global env var to a `tempfile::TempDir` so the
//!   skill discovery tests can run in parallel and stay
//!   isolated from the developer's actual user-level
//!   skills. One global mutex guards the swap so two tests
//!   cannot re-enter each other's `$HOME`.
//! - [`write_skill_md`] / [`empty_registry`] /
//!   [`empty_descriptor`] — tiny helpers the prompt
//!   assembler tests share.

use std::{
    path::Path,
    pin::Pin,
    sync::{Arc, LazyLock, Mutex, MutexGuard},
};

use futures::stream;
use synthia::{
    core::registry::RegistryItem,
    harness::{
        Agent,
        AgentDescriptor,
        AgentEntry,
        AgentEvent,
        AgentInput,
        AgentRegistry,
    },
};

/// A minimal agent that advertises only what the test
/// needs: a name and a description. `Agent::run` returns an
/// empty stream — the registry tests never call it.
pub(super) struct StubAgent {
    desc: AgentDescriptor,
}

impl RegistryItem for StubAgent {
    fn name(&self) -> &str {
        &self.desc.name
    }

    fn description(&self) -> &str {
        &self.desc.description
    }
}

#[async_trait::async_trait]
impl Agent for StubAgent {
    fn descriptor(&self) -> &AgentDescriptor {
        &self.desc
    }

    async fn run(
        &self,
        _input: AgentInput,
        _cancel: Arc<dyn synthia::core::CancelToken>,
    ) -> Pin<Box<dyn futures::Stream<Item = AgentEvent> + Send + 'static>> {
        Box::pin(stream::empty())
    }
}

/// Build a registry entry for `name`. Used by every test
/// that needs an agent on the books.
pub(super) fn stub_entry(name: &str) -> AgentEntry {
    AgentEntry::new(Arc::new(StubAgent {
        desc: AgentDescriptor {
            name: name.into(),
            description: format!("desc for {name}"),
            kind: "react".into(),
            version: "1.0.0".into(),
            instructions: String::new(),
            capabilities: vec!["tools".into()],
            tools: vec![],
            model_hint: None,
            handoffs: vec![],
            handoff_hint: None,
            output_schema: None,
            owner: None,
            domain: None,
            persona: None,
            display_name: None,
            max_iterations: None,
        },
    }))
}

/// Canonical descriptor the prompt assembler tests share:
/// every field at its zero value except the ones the
/// assembly contract cares about.
pub(super) fn empty_descriptor() -> AgentDescriptor {
    AgentDescriptor {
        name: "agent".into(),
        description: "ReAct loop".into(),
        kind: "react".into(),
        version: "1.0.0".into(),
        instructions: "BASE".into(),
        capabilities: Vec::new(),
        tools: Vec::new(),
        model_hint: None,
        handoffs: Vec::new(),
        handoff_hint: None,
        output_schema: None,
        owner: None,
        domain: None,
        persona: None,
        display_name: None,
        max_iterations: None,
    }
}

/// Build an empty `AgentRegistry` for tests that only need
/// the prompt assembler to receive an empty peer list.
pub(super) fn empty_registry() -> Arc<AgentRegistry> {
    Arc::new(AgentRegistry::new())
}

/// Write a `SKILL.md` at `<dir>/.agents/skills/<name>/SKILL.md`
/// with `content` verbatim (the test owns its own frontmatter
/// / body split). Returns the directory the skill lives in.
pub(super) fn write_skill_md(dir: &Path, name: &str, content: &str) {
    let skill_dir = dir.join(".agents").join("skills").join(name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
}

/// Pin `$HOME` to a fresh tempdir for the lifetime of the
/// returned guard. A process-global mutex serialises the
/// swap so two tests cannot race each other into reading
/// the developer's real `~/.claude/skills`.
pub(super) struct ScopedHome {
    // Fields are dropped in declaration order — so this
    // struct restores `$HOME` and tears down the tempdir
    // before the mutex is released. That ordering matters:
    // if the lock were released first, a sibling test
    // could observe `HOME` mid-restore and read from a
    // path that no longer exists.
    previous: Option<std::ffi::OsString>,
    home_dir: tempfile::TempDir,
    // Hold the global mutex for the guard's lifetime so
    // other tests cannot mutate `$HOME` while this guard
    // is alive.
    _guard: MutexGuard<'static, ()>,
}

impl ScopedHome {
    /// Lock the global HOME mutex and pin `$HOME` to a
    /// fresh tempdir. The returned guard's `Drop` releases
    /// the lock and restores `$HOME`.
    pub(super) fn new() -> Self {
        let home_dir = tempfile::tempdir().unwrap();
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var_os("HOME");
        // SAFETY: we hold the global mutex; no other thread
        // observes `$HOME` while we are mid-rewrite.
        unsafe {
            std::env::set_var("HOME", home_dir.path());
        }
        Self {
            previous,
            home_dir,
            _guard,
        }
    }

    pub(super) fn path(&self) -> &Path {
        self.home_dir.path()
    }
}

impl Drop for ScopedHome {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(v) => unsafe { std::env::set_var("HOME", v) },
            None => unsafe { std::env::remove_var("HOME") },
        }
    }
}

/// Process-global mutex that serialises every test
/// touching `$HOME`.
static HOME_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
