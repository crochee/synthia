//! File-backed scoped long-term memory (pi-subagents parity).
//!
//! [`FileMemory`] is the persistent long-term tier the [`Memory`]
//! trait was designed for: instead of an in-process vec that dies
//! with the run, entries live in per-scope directories that
//! survive restarts and are shared across sessions.
//!
//! # Scopes
//!
//! | Scope | Directory | Lifetime intent |
//! |---|---|---|
//! | [`MemoryScope::Local`] | `<root>/.synthia/memory-local/<agent>` | checkout-private, never shared |
//! | [`MemoryScope::Project`] | `<root>/.synthia/memory/<agent>` | checked in with the repository |
//! | [`MemoryScope::User`] | `<home>/.synthia/memory/<agent>` | shared across every project on the machine |
//!
//! [`FileMemoryConfig::scopes`] lists the scopes to search in
//! precedence order; [`MemoryScope::ALL`] is nearest-first
//! (local → project → user). Earlier scopes shadow later ones when
//! two files carry the same ULID, and new entries are written to
//! the **first** configured scope.
//!
//! # Layout
//!
//! ```text
//! <scope dir>/
//! ├── MEMORY.md            bounded index, regenerated on write
//! └── entries/
//!     └── <ulid>.md       frontmatter (name, description, type) + body
//! ```
//!
//! `MEMORY.md` is a projection of `entries/`, not a source of
//! truth: every [`FileMemory::store_entry`] writes the entry file
//! first and then re-renders the index from disk, so a crash
//! between the two leaves a recallable entry rather than a
//! dangling link. The index is capped at
//! [`MAX_MEMORY_INDEX_LINES`] lines (the reference implementation
//! truncates `MEMORY.md` at 200); entries beyond the cap are
//! summarised by a trailing omission note and remain recallable
//! from disk.
//!
//! # Recall
//!
//! [`FileMemory::search`] scores every entry against the query's
//! keywords — 3× for a hit in the name, 2× in the description, 1×
//! in the body — and returns hits by descending score, breaking
//! ties on descending id: ULIDs are time-sortable, so the newest
//! entry wins. The name and description are exactly the text the
//! index carries, so this is the deterministic, dependency-free
//! equivalent of searching the index plus the detail bodies. An
//! empty query matches every entry.
//!
//! # Defenses
//!
//! The tier mirrors the reference implementation's filesystem
//! posture. Agent names are whitelisted (`[A-Za-z0-9_-]`, at most
//! 128 characters — no separators, no dots, no `..`), every path
//! below the caller-supplied root is checked for symlinks before
//! it is read or created, and reads never create directories. A
//! symlinked scope directory, index, or entry file is refused with
//! [`FileMemoryError::Symlink`] rather than followed.
//!
//! # Tiers
//!
//! Only the long-term tier is file-backed. Conversation and working
//! memory delegate to an inner [`Memory`] ([`InMemoryMemory`] by
//! default, or any sink-backed impl via
//! [`FileMemory::with_inner`]), so a [`FileMemory`] is a complete
//! [`Memory`] implementation rather than a long-term-only shim.
//!
//! ## Module layout
//!
//! | Module | Responsibility |
//! |---|---|
//! | `scope` | [`MemoryScope`] — the three scopes and the directories they resolve to. |
//! | `entry` | [`MemoryKind`] + [`FileMemoryEntry`] — the frontmatter shape and its rendering. |
//! | `config` | [`FileMemoryConfig`] + the resolved per-scope directories. |
//! | `error` | [`FileMemoryError`]. |
//! | `guards` | The traversal defenses: agent-name whitelist, symlink refusal, contained walks. |
//! | `text` | Frontmatter / one-line text helpers shared by writer and reader. |
//! | `index` | Disk scan, keyword scoring, and the bounded `MEMORY.md` render. |
//! | `store` | [`FileMemory`] — the tier itself. |

/// Entries subdirectory name (one file per entry).
const ENTRIES_DIR: &str = "entries";
/// Index file name inside a scope directory.
const INDEX_FILE: &str = "MEMORY.md";
/// Hard cap on the rendered `MEMORY.md` index length; mirrors the
/// reference implementation's 200-line `MAX_MEMORY_LINES`.
pub const MAX_MEMORY_INDEX_LINES: usize = 200;
/// Longest accepted agent name.
const MAX_AGENT_NAME_LEN: usize = 128;

mod config;
mod entry;
mod error;
mod guards;
mod index;
mod scope;
mod store;
mod text;

pub use config::FileMemoryConfig;
pub use entry::{FileMemoryEntry, MemoryKind};
pub use error::FileMemoryError;
pub use scope::MemoryScope;
pub use store::FileMemory;

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink, path::Path, sync::Arc};

    use serde_json::json;
    use synthia_provider::Message;
    use synthia_session::in_memory::InMemorySessionSink;
    use tempfile::TempDir;
    use ulid::Ulid;

    use super::*;
    use crate::memory::{Memory, MemoryEntry, MemoryError};

    fn temp() -> TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    fn config(root: &Path, home: &Path) -> FileMemoryConfig {
        FileMemoryConfig::new(root, "tester")
            .with_user_base(home)
            .with_scopes(MemoryScope::ALL.to_vec())
    }

    fn memory(root: &Path, home: &Path) -> FileMemory {
        FileMemory::new(config(root, home)).expect("file memory")
    }

    fn name_of(entry: &MemoryEntry) -> String {
        let metadata = entry.metadata.clone().expect("metadata");
        metadata["name"].as_str().expect("name").to_string()
    }

    // -- scope resolution + precedence -------------------------------

    #[test]
    fn scope_dirs_follow_the_reference_layout() {
        let root = temp();
        let home = temp();
        let mem = memory(root.path(), home.path());
        assert_eq!(
            mem.dir(MemoryScope::Local),
            Some(root.path().join(".synthia/memory-local/tester").as_path())
        );
        assert_eq!(
            mem.dir(MemoryScope::Project),
            Some(root.path().join(".synthia/memory/tester").as_path())
        );
        assert_eq!(
            mem.dir(MemoryScope::User),
            Some(home.path().join(".synthia/memory/tester").as_path())
        );
        assert_eq!(
            MemoryScope::ALL,
            [MemoryScope::Local, MemoryScope::Project, MemoryScope::User]
        );
        assert_eq!(MemoryScope::parse("project"), Some(MemoryScope::Project));
        assert_eq!(MemoryScope::parse("nope"), None);
        assert_eq!(MemoryKind::parse("feedback"), Some(MemoryKind::Feedback));
        assert_eq!(MemoryKind::parse("nope"), None);
    }

    #[test]
    fn unconfigured_scopes_resolve_to_none() {
        let root = temp();
        let mem = FileMemory::new(
            FileMemoryConfig::new(root.path(), "tester")
                .with_scopes(vec![MemoryScope::Project]),
        )
        .expect("file memory");
        assert_eq!(
            mem.dir(MemoryScope::Project),
            Some(root.path().join(".synthia/memory/tester").as_path())
        );
        assert_eq!(mem.dir(MemoryScope::Local), None);
        assert_eq!(mem.dir(MemoryScope::User), None);
    }

    #[test]
    fn agent_names_outside_the_whitelist_are_refused() {
        let root = temp();
        let long = "x".repeat(MAX_AGENT_NAME_LEN + 1);
        let bad = [
            "",
            "..",
            "../etc",
            "a/b",
            "a.b",
            "a b",
            ".hidden",
            long.as_str(),
        ];
        for name in bad {
            let config = FileMemoryConfig::new(root.path(), name)
                .with_scopes(vec![MemoryScope::Project]);
            let error =
                FileMemory::new(config).expect_err("must refuse the name");
            assert!(
                matches!(error, FileMemoryError::InvalidAgentName(_)),
                "unexpected error for {name:?}: {error}"
            );
        }
        let ok = FileMemoryConfig::new(root.path(), "agent-1_X")
            .with_scopes(vec![MemoryScope::Project]);
        assert!(FileMemory::new(ok).is_ok());
    }

    #[test]
    fn configuration_refusals_are_typed() {
        let root = temp();
        let error = FileMemory::new(
            FileMemoryConfig::new(root.path(), "tester")
                .with_scopes(Vec::new()),
        )
        .expect_err("must refuse an empty scope list");
        assert_eq!(error, FileMemoryError::NoScopes);
    }

    #[tokio::test]
    async fn nearer_scopes_shadow_later_ones() {
        let root = temp();
        let home = temp();
        let id = Ulid::from(7u128).to_string();
        let write_to = |scope: MemoryScope| {
            FileMemory::new(
                config(root.path(), home.path()).with_scopes(vec![scope]),
            )
            .expect("file memory")
        };
        let local = write_to(MemoryScope::Local);
        local
            .store(MemoryEntry::now(id.clone(), "alpha from local"))
            .await
            .expect("store local");
        let project = write_to(MemoryScope::Project);
        project
            .store(MemoryEntry::now(id.clone(), "alpha from project"))
            .await
            .expect("store project");
        // The default scope list is [Local, Project, User]: the
        // nearest copy wins on the id collision.
        let overlayed = memory(root.path(), home.path());
        let hits = overlayed.search("alpha", 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "alpha from local");
        let metadata = hits[0].metadata.clone().expect("metadata");
        assert_eq!(metadata["scope"], "local");
        // A project-only view sees its own copy.
        assert_eq!(
            project.search("alpha", 10).expect("search")[0].content,
            "alpha from project"
        );
    }

    // -- store / recall ---------------------------------------------

    #[test]
    fn store_recall_round_trip() {
        let root = temp();
        let home = temp();
        let mem = memory(root.path(), home.path());
        let id = mem
            .store_entry(&FileMemoryEntry {
                name: "Build steps".to_string(),
                description: "How to build the workspace".to_string(),
                kind: MemoryKind::Project,
                content: "Run cargo build.\nThen cargo test.".to_string(),
            })
            .expect("store");
        assert!(Ulid::from_string(&id).is_ok(), "not a ulid: {id}");
        let hits = mem.search("cargo", 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, id);
        assert_eq!(hits[0].content, "Run cargo build.\nThen cargo test.");
        let metadata = hits[0].metadata.clone().expect("metadata");
        assert_eq!(metadata["name"], "Build steps");
        assert_eq!(metadata["description"], "How to build the workspace");
        assert_eq!(metadata["type"], "project");
        assert_eq!(metadata["scope"], "local");
        // A fresh tier over the same directories sees the entry:
        // the memory is on disk, not in the instance.
        let reopened = memory(root.path(), home.path());
        assert_eq!(reopened.search("cargo", 10).expect("search").len(), 1);
    }

    #[test]
    fn recall_ranks_name_above_description_above_body() {
        let root = temp();
        let home = temp();
        let mem = memory(root.path(), home.path());
        let entry =
            |name: &str, description: &str, content: &str| FileMemoryEntry {
                name: name.to_string(),
                description: description.to_string(),
                kind: MemoryKind::Reference,
                content: content.to_string(),
            };
        mem.store_entry(&entry("alpha-notes", "notes", "unrelated body"))
            .expect("store name hit");
        mem.store_entry(&entry("notes-b", "alpha notes", "unrelated body"))
            .expect("store description hit");
        mem.store_entry(&entry("notes-c", "notes", "alpha body"))
            .expect("store body hit");
        mem.store_entry(&entry("notes-d", "notes", "nothing at all"))
            .expect("store miss");

        let hits = mem.search("alpha", 10).expect("search");
        let names: Vec<String> = hits.iter().map(name_of).collect();
        assert_eq!(names, ["alpha-notes", "notes-b", "notes-c"]);
        assert_eq!(mem.search("alpha", 2).expect("search").len(), 2);
        assert!(mem.search("zzz", 10).expect("search").is_empty());
    }

    #[tokio::test]
    async fn score_ties_break_toward_the_newest_id() {
        let root = temp();
        let home = temp();
        let mem = memory(root.path(), home.path());
        let older = Ulid::from(1u128).to_string();
        let newer = Ulid::from(2u128).to_string();
        mem.store(MemoryEntry::now(older.clone(), "alpha older"))
            .await
            .expect("store older");
        mem.store(MemoryEntry::now(newer.clone(), "alpha newer"))
            .await
            .expect("store newer");
        let ids: Vec<String> = mem
            .search("alpha", 10)
            .expect("search")
            .iter()
            .map(|entry| entry.id.clone())
            .collect();
        assert_eq!(ids, [newer, older]);
    }

    #[test]
    fn empty_query_matches_everything_up_to_the_limit() {
        let root = temp();
        let home = temp();
        let mem = memory(root.path(), home.path());
        for index in 0..3 {
            mem.store_entry(&FileMemoryEntry::new(
                format!("note-{index}"),
                format!("body {index}"),
            ))
            .expect("store");
        }
        assert_eq!(mem.search("", 10).expect("search").len(), 3);
        assert_eq!(mem.search("", 2).expect("search").len(), 2);
        assert_eq!(mem.search("", 0).expect("search").len(), 0);
        assert_eq!(mem.search("absent", 10).expect("search").len(), 0);
    }

    #[tokio::test]
    async fn trait_store_maps_metadata_into_frontmatter() {
        let root = temp();
        let home = temp();
        let mem = memory(root.path(), home.path());
        let mut entry = MemoryEntry::now("", "alpha body\nsecond line");
        entry.metadata = Some(json!({
            "name": "From metadata",
            "description": "desc here",
            "type": "feedback",
        }));
        mem.store(entry).await.expect("store");

        let hits = mem.search("alpha", 10).expect("search");
        assert_eq!(hits.len(), 1);
        let metadata = hits[0].metadata.clone().expect("metadata");
        assert_eq!(metadata["name"], "From metadata");
        assert_eq!(metadata["description"], "desc here");
        assert_eq!(metadata["type"], "feedback");
        assert_eq!(hits[0].content, "alpha body\nsecond line");
        let raw = fs::read_to_string(
            mem.dir(MemoryScope::Local)
                .expect("primary dir")
                .join(ENTRIES_DIR)
                .join(format!("{}.md", hits[0].id)),
        )
        .expect("entry file");
        assert!(raw.contains("name: From metadata"), "{raw}");
        assert!(raw.contains("type: feedback"), "{raw}");
    }

    #[test]
    fn entries_without_frontmatter_are_tolerated_and_foreign_names_ignored() {
        let root = temp();
        let home = temp();
        let mem = memory(root.path(), home.path());
        let entries = mem
            .dir(MemoryScope::Local)
            .expect("primary dir")
            .join(ENTRIES_DIR);
        fs::create_dir_all(&entries).expect("entries dir");
        fs::write(
            entries.join("notes.md"),
            "---\nname: hand written\n---\n\nalpha body\n",
        )
        .expect("foreign entry");
        let id = Ulid::from(42u128).to_string();
        fs::write(
            entries.join(format!("{id}.md")),
            "alpha without frontmatter\nmore text\n",
        )
        .expect("bare entry");

        let hits = mem.search("alpha", 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, id);
        assert_eq!(hits[0].content, "alpha without frontmatter\nmore text");
        let metadata = hits[0].metadata.clone().expect("metadata");
        assert_eq!(metadata["name"], "alpha without frontmatter");
        assert_eq!(metadata["type"], "project");
    }

    // -- index ------------------------------------------------------

    #[test]
    fn reads_never_create_the_memory_directory() {
        let root = temp();
        let home = temp();
        let mem = memory(root.path(), home.path());
        assert!(mem.search("anything", 5).expect("search").is_empty());
        assert!(mem.index().expect("index").contains("Agent Memory"));
        assert!(mem.prompt_block().expect("block").contains("No memories"));
        assert!(!root.path().join(".synthia").exists());
    }

    #[test]
    fn store_refreshes_the_index() {
        let root = temp();
        let home = temp();
        let mem = memory(root.path(), home.path());
        let index_path = mem
            .dir(MemoryScope::Local)
            .expect("primary dir")
            .join(INDEX_FILE);
        assert!(!index_path.exists());
        let first = mem
            .store_entry(&FileMemoryEntry::new("first note", "body one"))
            .expect("store first");
        let index = fs::read_to_string(&index_path).expect("index");
        assert!(index.contains("first note"), "{index}");
        assert!(index.contains(&format!("entries/{first}.md")), "{index}");
        let second = mem
            .store_entry(&FileMemoryEntry::new("second note", "body two"))
            .expect("store second");
        let index = fs::read_to_string(&index_path).expect("index");
        assert!(index.contains(&format!("entries/{first}.md")), "{index}");
        assert!(index.contains(&format!("entries/{second}.md")), "{index}");
        assert_eq!(index.lines().count(), 3);
        assert_eq!(index, mem.index().expect("index"));
    }

    #[test]
    fn index_is_bounded_to_the_line_cap() {
        let root = temp();
        let home = temp();
        let mem = memory(root.path(), home.path());
        for index in 0..(MAX_MEMORY_INDEX_LINES + 5) {
            mem.store_entry(&FileMemoryEntry::new(
                format!("note {index}"),
                format!("body {index}"),
            ))
            .expect("store");
        }
        let index = mem.index().expect("index");
        assert!(
            index.lines().count() <= MAX_MEMORY_INDEX_LINES,
            "index grew to {} lines",
            index.lines().count()
        );
        assert!(index.contains("older entries omitted"), "{index}");
        let disk = fs::read_to_string(
            mem.dir(MemoryScope::Local)
                .expect("primary dir")
                .join(INDEX_FILE),
        )
        .expect("index");
        assert!(disk.lines().count() <= MAX_MEMORY_INDEX_LINES);
        // Every dropped entry is still recallable from disk.
        assert_eq!(
            mem.search("note", MAX_MEMORY_INDEX_LINES + 5)
                .expect("search")
                .len(),
            MAX_MEMORY_INDEX_LINES + 5
        );
    }

    #[test]
    fn prompt_block_carries_scope_directory_and_index() {
        let root = temp();
        let home = temp();
        let mem = memory(root.path(), home.path());
        let id = mem
            .store_entry(&FileMemoryEntry::new("build notes", "alpha body"))
            .expect("store");
        let block = mem.prompt_block().expect("block");
        assert!(block.contains("build notes"), "{block}");
        assert!(block.contains(&format!("entries/{id}.md")), "{block}");
        assert!(block.contains(".synthia/memory-local/tester"), "{block}");
        assert!(block.contains("Memory scope: local"), "{block}");
        assert!(!block.contains("read-only"), "{block}");

        let frozen = FileMemory::new(
            config(root.path(), home.path()).with_read_only(true),
        )
        .expect("file memory");
        let block = frozen.prompt_block().expect("block");
        assert!(block.contains("read-only"), "{block}");
        assert!(block.contains("do not create or modify"), "{block}");
    }

    // -- defenses ---------------------------------------------------

    #[tokio::test]
    async fn read_only_refuses_writes_and_keeps_recall() {
        let root = temp();
        let home = temp();
        let writable = memory(root.path(), home.path());
        writable
            .store_entry(&FileMemoryEntry::new("note", "alpha body"))
            .expect("store");
        let frozen = FileMemory::new(
            config(root.path(), home.path()).with_read_only(true),
        )
        .expect("file memory");
        assert!(frozen.is_read_only());
        assert_eq!(frozen.search("alpha", 5).expect("search").len(), 1);
        let error = frozen
            .store_entry(&FileMemoryEntry::new("nope", "beta"))
            .expect_err("write must be refused");
        assert!(matches!(error, FileMemoryError::ReadOnly(_)));
        let stored = frozen.store(MemoryEntry::now("", "beta")).await;
        assert!(matches!(
            stored,
            Err(MemoryError::File(FileMemoryError::ReadOnly(_)))
        ));

        // A read-only tier over a fresh root creates nothing.
        let fresh = temp();
        let untouched = FileMemory::new(
            FileMemoryConfig::new(fresh.path(), "tester")
                .with_scopes(vec![MemoryScope::Project])
                .with_read_only(true),
        )
        .expect("file memory");
        assert!(
            untouched
                .store_entry(&FileMemoryEntry::new("nope", "beta"))
                .is_err()
        );
        assert!(!fresh.path().join(".synthia").exists());
    }

    #[test]
    fn symlinked_scope_directory_is_refused() {
        let root = temp();
        let home = temp();
        let elsewhere = temp();
        fs::create_dir_all(root.path().join(".synthia")).expect("synthia dir");
        symlink(elsewhere.path(), root.path().join(".synthia/memory-local"))
            .expect("symlink");
        let mem = memory(root.path(), home.path());
        let write = mem
            .store_entry(&FileMemoryEntry::new("note", "alpha body"))
            .expect_err("write must be refused");
        assert!(matches!(write, FileMemoryError::Symlink(_)), "{write}");
        let read = mem.search("alpha", 5).expect_err("read must be refused");
        assert!(matches!(read, FileMemoryError::Symlink(_)), "{read}");
        assert!(
            !elsewhere.path().join("tester").exists(),
            "nothing may be written through the symlink"
        );
    }

    #[test]
    fn symlinked_entry_and_index_files_are_refused() {
        let root = temp();
        let home = temp();
        let mem = memory(root.path(), home.path());
        let id = mem
            .store_entry(&FileMemoryEntry::new("note", "alpha body"))
            .expect("store");
        let scope = mem.dir(MemoryScope::Local).expect("primary dir");
        let outside = root.path().join("outside.md");
        fs::write(&outside, "---\nname: outside\n---\n\nalpha\n")
            .expect("outside file");

        let entry = scope.join(ENTRIES_DIR).join(format!("{id}.md"));
        fs::remove_file(&entry).expect("remove entry");
        symlink(&outside, &entry).expect("symlink entry");
        let read = mem.search("alpha", 5).expect_err("read must be refused");
        assert!(matches!(read, FileMemoryError::Symlink(_)), "{read}");
        fs::remove_file(&entry).expect("remove symlink");

        let index = scope.join(INDEX_FILE);
        fs::remove_file(&index).expect("remove index");
        symlink(&outside, &index).expect("symlink index");
        let write = mem
            .store_entry(&FileMemoryEntry::new("another", "beta"))
            .expect_err("write must be refused");
        assert!(matches!(write, FileMemoryError::Symlink(_)), "{write}");
        assert_eq!(
            fs::read_to_string(&outside).expect("outside intact"),
            "---\nname: outside\n---\n\nalpha\n"
        );
    }

    // -- delegation --------------------------------------------------

    #[tokio::test]
    async fn conversation_and_working_tiers_delegate_to_the_inner() {
        let root = temp();
        let home = temp();
        let mem = memory(root.path(), home.path());
        let session = mem.create_session().await.expect("session");
        mem.append(&session, Message::user("hello"))
            .await
            .expect("append");
        assert_eq!(mem.messages(&session).await.expect("messages").len(), 1);
        mem.set_context(&session, "task", json!("ship it"))
            .await
            .expect("set");
        assert_eq!(
            mem.get_context(&session, "task").await.expect("get"),
            Some(json!("ship it"))
        );
        assert_eq!(
            mem.list_sessions().await.expect("list"),
            vec![session.clone()]
        );
        mem.delete_session(&session).await.expect("delete");
        assert!(mem.messages(&session).await.expect("messages").is_empty());
    }

    #[tokio::test]
    async fn supplied_inner_memory_backs_the_volatile_tiers() {
        let root = temp();
        let home = temp();
        let sink = Arc::new(InMemorySessionSink::new("s1"));
        let inner: Arc<dyn Memory> =
            Arc::new(super::super::SessionMemory::new(sink));
        let mem =
            FileMemory::with_inner(config(root.path(), home.path()), inner)
                .expect("file memory");
        mem.append("s1", Message::user("through the sink"))
            .await
            .expect("append");
        assert_eq!(
            mem.messages("s1").await.expect("messages"),
            vec![Message::user("through the sink")]
        );
    }
}
