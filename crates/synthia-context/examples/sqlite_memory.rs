//! `sqlite_memory` — the three memory tiers in one `SQLite` file.
//!
//! Seam: `synthia_context::memory::SqliteMemory` (feature `sqlite`,
//! `crates/synthia-context/src/memory/sqlite.rs`).
//!
//! Look at: `in_memory()` for long-term `store` / `recall` (an FTS5
//! query ordered by BM25, best first), `set_context` / `get_context`
//! and `create_session` / `list_sessions` for the working-memory and
//! session tiers, then a read-only open that keeps recall working
//! while every write is refused.
//!
//! Run (no network, no API key):
//!
//! ```bash
//! cargo run -p synthia-context --features sqlite --example sqlite_memory
//! ```

use serde_json::json;
use synthia_context::memory::{
    Memory,
    MemoryEntry,
    MemoryError,
    SqliteMemory,
    SqliteMemoryError,
};

/// Fixed ULIDs, so the stored rows and their ids are deterministic.
const ID_A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const ID_B: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
const ID_C: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAX";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let memory = SqliteMemory::in_memory().expect("open an in-memory db");
    println!("== synthia sqlite memory ==\n");
    println!(
        "in-memory db                : path={:?}, read_only={}",
        memory.path(),
        memory.is_read_only()
    );

    // 1. Long-term tier: three entries, one of which matches every
    //    term of the query, so BM25 must put it first.
    for (id, content) in [
        (ID_A, "sqlite fts5 bm25 ranking"),
        (ID_B, "sqlite backups with vacuum"),
        (ID_C, "banana bread recipe"),
    ] {
        memory
            .store(MemoryEntry::now(id, content))
            .await
            .expect("store a long-term entry");
    }

    let hits = memory
        .recall("sqlite bm25 ranking", 5)
        .await
        .expect("recall long-term entries");
    println!("\n[long-term] recall(\"sqlite bm25 ranking\")");
    for (rank, hit) in hits.iter().enumerate() {
        println!("  #{} {}", rank + 1, hit.content);
    }
    println!("hit count                   : {}", hits.len());
    println!(
        "best match first            : {}",
        hits.first().map(|hit| hit.content.as_str())
            == Some("sqlite fts5 bm25 ranking")
    );

    // 2. Working-memory tier, scoped to a session the registry mints.
    let session = memory.create_session().await.expect("create session");
    memory
        .set_context(&session, "task", json!({"step": 2}))
        .await
        .expect("write working memory");
    let task = memory
        .get_context(&session, "task")
        .await
        .expect("read working memory");
    println!("\n[working] session           : {session}");
    println!("[working] task              : {task:?}");
    println!(
        "[sessions] listed           : {:?}",
        memory.list_sessions().await.expect("list sessions")
    );

    // 3. Read-only tier on a real file: recall still works, writes
    //    are refused with the typed `ReadOnly` error.
    let dir = tempfile::tempdir().expect("create a temp dir");
    let path = dir.path().join("memory.db");
    {
        let writable = SqliteMemory::open(&path).expect("open the db file");
        writable
            .store(MemoryEntry::now(ID_A, "durable fact"))
            .await
            .expect("store a durable entry");
    }
    let reader = SqliteMemory::open_read_only(&path).expect("reopen read-only");
    let recalled = reader
        .recall("durable", 5)
        .await
        .expect("recall from a read-only db");
    let refused = reader.store(MemoryEntry::now(ID_B, "rejected")).await;
    println!("\n[read-only] path            : {}", path.display());
    println!("[read-only] is_read_only    : {}", reader.is_read_only());
    println!("[read-only] recall hits     : {}", recalled.len());
    match refused {
        Err(MemoryError::Sqlite(SqliteMemoryError::ReadOnly)) => {
            println!("[read-only] store           : refused (ReadOnly)");
        }
        other => println!("[read-only] store           : UNEXPECTED {other:?}"),
    }

    println!("\nSQLITE-MEMORY: OK");
}
