//! Snapshot + op-log persistence for [`SearchEngine`].
//!
//! Layout under `root`:
//!
//! - `snapshot.json` — the full alive set (`{ item, vector }` per doc),
//!   written atomically (`.tmp` + rename) by [`SearchEngine::save`] and
//!   by the compact hook.
//! - `ops.jsonl` — append-only `add` / `remove` records since the last
//!   snapshot; one JSON object per line.
//!
//! [`SearchEngine::load`] = snapshot + op replay. Vectors are
//! persisted, so non-deterministic embedders (e.g. provider-backed)
//! restore without re-embedding; BM25 postings are rebuilt from
//! [`Searchable::indexed_fields`]
//! (deterministic, cheap) and never hit disk.
//!
//! The methods live on a dedicated impl block gated on
//! `T: Serialize + DeserializeOwned`: a `SearchEngine<T>` over a
//! non-serde `T` simply has no `save` / `load`.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Serialize, de::DeserializeOwned};
use tracing::warn;

use crate::{
    SearchEngine,
    embedder::Embedder,
    engine::EngineInner,
    error::{Result, SearchError},
    searchable::Searchable,
    tokenizer::Tokenizer,
};

const SNAPSHOT_FILE: &str = "snapshot.json";
const OPS_FILE: &str = "ops.jsonl";

/// One alive entry in `snapshot.json`.
#[derive(Serialize, serde::Deserialize)]
pub(crate) struct SnapshotDoc<T> {
    item: T,
    vector: Vec<f32>,
}

/// One line of `ops.jsonl`.
#[derive(Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Op<T> {
    Add { item: T, vector: Vec<f32> },
    Remove { name: String },
}

/// What the engine asks persistence to do. `add` / `compact` live on
/// `impl<T: Searchable> SearchEngine<T>`, where `T: Serialize` cannot
/// be proven, so they reach the disk through this type-erased hook
/// instead of calling the serde-bounded code directly.
pub(crate) enum PersistCmd<T: Searchable> {
    /// Append an `Op::Add` line to `ops.jsonl`.
    AppendAdd {
        root: PathBuf,
        item: T,
        vector: Vec<f32>,
    },
    /// Rewrite `snapshot.json` from `docs` and truncate `ops.jsonl`
    /// (the compact hook).
    RewriteSnapshot {
        root: PathBuf,
        docs: Vec<SnapshotDoc<T>>,
    },
}

/// Best-effort disk writer installed on the engine by `save` / `load`;
/// failures `warn!` and never fail the engine mutation.
pub(crate) type PersistHook<T> = Arc<dyn Fn(PersistCmd<T>) + Send + Sync>;

impl<T: Searchable + Serialize + DeserializeOwned> SearchEngine<T> {
    /// Persist the alive set: full snapshot first, then append-only ops.
    ///
    /// The write lock is held across the file writes, so the cut is
    /// atomic — no interleaved `add` can append an op that the trailing
    /// truncate would then drop. On success the engine starts
    /// op-logging to `root`.
    ///
    /// # Errors
    ///
    /// [`SearchError::Persist`] on any filesystem or serialisation
    /// failure; the in-memory engine is never modified by `save`.
    pub fn save(&self, root: &Path) -> Result<()> {
        fs::create_dir_all(root).map_err(io_persist)?;
        let mut g = self.inner.write();
        let docs = collect_alive(&g);
        write_snapshot(root, &docs)?;
        g.persist_root = Some(root.to_path_buf());
        g.persist_hook = Some(make_persist_hook::<T>());
        Ok(())
    }

    /// Rebuild from `root`: snapshot + op replay. The embedder is stored for
    /// future adds; restored docs never re-embed (vectors come from disk).
    ///
    /// A missing `snapshot.json` replays `ops.jsonl` on its own — an
    /// empty tail when that file is missing too, which is the first
    /// boot: `load` on a fresh root then `add` without `save` /
    /// `compact` only ever wrote ops, and the next boot must see them.
    /// Malformed op lines are skipped with a
    /// `warn!`, mirroring `ScheduleStore::load`; a malformed snapshot,
    /// or any vector whose length disagrees with `embedder.dim()`, is a
    /// hard error. Ops recorded during replay are not re-appended: the
    /// hook is installed only after the whole state is restored.
    ///
    /// # Errors
    ///
    /// [`SearchError::Persist`] on filesystem / JSON failure and
    /// [`SearchError::DimMismatch`] when a persisted vector's length
    /// disagrees with the embedder.
    pub fn load(
        root: &Path,
        embedder: Arc<dyn Embedder>,
        tokenizer: Arc<dyn Tokenizer>,
    ) -> Result<Self> {
        let expected = embedder.dim();
        let engine = Self::new(embedder, tokenizer);
        let snapshot = root.join(SNAPSHOT_FILE);
        if !snapshot.exists() {
            fs::create_dir_all(root).map_err(io_persist)?;
            // No snapshot: `ops.jsonl` is the whole state — a root that
            // was only ever `load`ed (never `save`d / compacted) has
            // nothing else. Replay before installing the hook, or the
            // tail is discarded and the next save truncates it.
            replay_ops(&engine, &root.join(OPS_FILE), expected)?;
            install_persistence(&engine, root);
            return Ok(engine);
        }
        let raw = fs::read_to_string(&snapshot).map_err(io_persist)?;
        let docs: Vec<SnapshotDoc<T>> = serde_json::from_str(&raw)
            .map_err(|err| SearchError::Persist(err.to_string()))?;
        for doc in docs {
            check_dim(&doc.vector, expected)?;
            engine.add_with_vector(doc.item, doc.vector)?;
        }
        replay_ops(&engine, &root.join(OPS_FILE), expected)?;
        install_persistence(&engine, root);
        Ok(engine)
    }
}

/// Build the type-erased hook. The hook is the *best-effort* half of
/// persistence: `save` / `load` call the fallible cores directly so
/// they can report errors, while the engine's hot paths go through
/// here and only `warn!` on failure.
pub(crate) fn make_persist_hook<T>() -> PersistHook<T>
where
    T: Searchable + Serialize,
{
    Arc::new(|cmd| {
        let (root, result) = match cmd {
            PersistCmd::AppendAdd { root, item, vector } => {
                let result = append_op(&root, &Op::Add { item, vector });
                (root, result)
            }
            PersistCmd::RewriteSnapshot { root, docs } => {
                let result = write_snapshot(&root, &docs);
                (root, result)
            }
        };
        if let Err(err) = result {
            warn!(
                root = %root.display(),
                error = %err,
                "search persist: background write failed; engine state is unaffected",
            );
        }
    })
}

/// Append an `Op::Remove` line. A free function rather than a hook
/// command: the remove op carries no `T`, so the generic `remove` path
/// can build it without the `Serialize` bound.
pub(crate) fn append_remove_op(root: &Path, name: &str) {
    // Mirrors `Op::Remove` on the wire (`{"op":"remove","name":…}`).
    let mut line =
        serde_json::json!({ "op": "remove", "name": name }).to_string();
    line.push('\n');
    let written = OpenOptions::new()
        .append(true)
        .create(true)
        .open(root.join(OPS_FILE))
        .and_then(|mut file| file.write_all(line.as_bytes()));
    if let Err(err) = written {
        warn!(
            root = %root.display(),
            error = %err,
            "search persist: remove op dropped; engine state is unaffected",
        );
    }
}

/// Collect the alive set for a snapshot, ordered by slot index so the
/// file bytes are deterministic for a given engine state.
pub(crate) fn collect_alive<T: Searchable>(
    g: &EngineInner<T>,
) -> Vec<SnapshotDoc<T>> {
    let mut slots: Vec<usize> = g.id_to_idx.values().copied().collect();
    slots.sort_unstable();
    slots
        .into_iter()
        .map(|idx| SnapshotDoc {
            item: g.items[idx].clone(),
            vector: g.vectors.vector_at(idx).expect("alive 必有向量"),
        })
        .collect()
}

/// Set `persist_root` + install the hook. Called at the end of a
/// successful `save` / `load` — after replay, so restored ops are never
/// re-appended to the log.
fn install_persistence<T>(engine: &SearchEngine<T>, root: &Path)
where
    T: Searchable + Serialize,
{
    let mut g = engine.inner.write();
    g.persist_root = Some(root.to_path_buf());
    g.persist_hook = Some(make_persist_hook::<T>());
}

/// Write `snapshot.json` atomically and truncate `ops.jsonl`: the
/// snapshot covers every alive doc, so the op tail restarts empty.
fn write_snapshot<T: Serialize>(
    root: &Path,
    docs: &[SnapshotDoc<T>],
) -> Result<()> {
    let json = serde_json::to_string(docs)
        .map_err(|err| SearchError::Persist(err.to_string()))?;
    let tmp = root.join(format!("{SNAPSHOT_FILE}.tmp"));
    fs::write(&tmp, json).map_err(io_persist)?;
    fs::rename(&tmp, root.join(SNAPSHOT_FILE)).map_err(io_persist)?;
    fs::write(root.join(OPS_FILE), "").map_err(io_persist)?;
    Ok(())
}

/// Append one op line to `ops.jsonl`.
fn append_op<T: Serialize>(root: &Path, op: &Op<T>) -> Result<()> {
    let mut line = serde_json::to_string(op)
        .map_err(|err| SearchError::Persist(err.to_string()))?;
    line.push('\n');
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(root.join(OPS_FILE))
        .map_err(io_persist)?;
    file.write_all(line.as_bytes()).map_err(io_persist)?;
    Ok(())
}

/// Replay the op tail after the snapshot. Malformed lines are skipped
/// with a `warn!`; a vector that disagrees with the embedder dim is a
/// hard error (data/config drift, same contract as the snapshot). A
/// missing file is an empty tail — the first boot, where neither the
/// snapshot nor the log exists yet.
fn replay_ops<T>(
    engine: &SearchEngine<T>,
    ops: &Path,
    expected: usize,
) -> Result<()>
where
    T: Searchable + Serialize + DeserializeOwned,
{
    let raw = match fs::read_to_string(ops) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(());
        }
        Err(err) => return Err(io_persist(err)),
    };
    for (lineno, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let lineno = lineno + 1;
        match serde_json::from_str::<Op<T>>(line) {
            Ok(Op::Add { item, vector }) => {
                check_dim(&vector, expected)?;
                if let Err(err) = engine.add_with_vector(item, vector) {
                    warn!(
                        op_line = lineno,
                        error = %err,
                        "search persist: add replay failed, skipping",
                    );
                }
            }
            Ok(Op::Remove { name }) => {
                let _ = engine.remove(&name); // idempotent by design
            }
            Err(err) => {
                warn!(
                    op_line = lineno,
                    error = %err,
                    "search persist: skipping malformed op line",
                );
            }
        }
    }
    Ok(())
}

fn check_dim(vector: &[f32], expected: usize) -> Result<()> {
    if vector.len() != expected {
        return Err(SearchError::DimMismatch {
            expected,
            got: vector.len(),
        });
    }
    Ok(())
}

fn io_persist(err: std::io::Error) -> SearchError {
    SearchError::Persist(err.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde::Serialize;
    use synthia_core::registry::RegistryItem;

    use crate::{
        CjkTokenizer,
        Embedder,
        HashingEmbedder,
        QueryContext,
        SearchEngine,
        Searchable,
        Tokenizer,
    };

    #[derive(Clone, Serialize, serde::Deserialize)]
    struct Doc {
        id: String,
        title: String,
        body: String,
    }

    impl RegistryItem for Doc {
        fn name(&self) -> &str {
            &self.id
        }

        fn description(&self) -> &str {
            &self.title
        }
    }

    impl Searchable for Doc {
        fn indexed_fields(&self) -> Vec<(String, f32)> {
            vec![(self.title.clone(), 2.0), (self.body.clone(), 1.0)]
        }
    }

    fn engine(
        dim: usize,
    ) -> (Arc<dyn Tokenizer>, Arc<dyn Embedder>, SearchEngine<Doc>) {
        let tk: Arc<dyn Tokenizer> = Arc::new(CjkTokenizer);
        let emb: Arc<dyn Embedder> =
            Arc::new(HashingEmbedder::new(dim, tk.clone()));
        (tk.clone(), emb.clone(), SearchEngine::new(emb, tk))
    }

    #[test]
    fn roundtrip_preserves_search_results() {
        let dir = tempfile::tempdir().unwrap();
        let (_, _, e) = engine(32);
        e.add(Doc {
            id: "d1".into(),
            title: "release notes".into(),
            body: "wheel landed".into(),
        })
        .unwrap();
        e.add(Doc {
            id: "d2".into(),
            title: "cron parser".into(),
            body: "trigger seam".into(),
        })
        .unwrap();
        let before = e.search(&QueryContext::new("release notes"));
        e.save(dir.path()).unwrap();
        let (tk, emb, _) = engine(32);
        let restored = SearchEngine::<Doc>::load(dir.path(), emb, tk).unwrap();
        let after = restored.search(&QueryContext::new("release notes"));
        assert_eq!(before.len(), after.len());
        assert_eq!(after.first().map(|h| h.id.clone()), Some("d1".into()));
    }

    #[test]
    fn removed_doc_does_not_resurrect() {
        let dir = tempfile::tempdir().unwrap();
        let (_, _, e) = engine(32);
        e.add(Doc {
            id: "d1".into(),
            title: "t".into(),
            body: "b".into(),
        })
        .unwrap();
        e.remove("d1");
        e.save(dir.path()).unwrap();
        let (tk, emb, _) = engine(32);
        let restored = SearchEngine::<Doc>::load(dir.path(), emb, tk).unwrap();
        assert!(restored.get("d1").is_none());
        assert_eq!(restored.len(), 0);
    }

    #[test]
    fn oplog_tail_is_replayed_after_snapshot() {
        // save → 再 add 一条 → load 能看到（证明 ops.jsonl 生效）
        let dir = tempfile::tempdir().unwrap();
        let (_, _, e) = engine(32);
        e.add(Doc {
            id: "d1".into(),
            title: "a".into(),
            body: "x".into(),
        })
        .unwrap();
        e.save(dir.path()).unwrap();
        e.add(Doc {
            id: "d2".into(),
            title: "b".into(),
            body: "y".into(),
        })
        .unwrap();
        let (tk, emb, _) = engine(32);
        let restored = SearchEngine::<Doc>::load(dir.path(), emb, tk).unwrap();
        assert_eq!(restored.len(), 2);
        assert!(restored.get("d2").is_some());
    }

    #[test]
    fn oplog_tail_replays_when_no_snapshot_was_ever_written() {
        // load(空 root) 装上持久化 → add 只写 ops.jsonl；没 save/compact
        // 就重启：snapshot 不存在，尾部也必须重放（否则整段落丢，且
        // 下一次 save 会把它截断）。
        let dir = tempfile::tempdir().unwrap();
        let (tk, emb, _) = engine(32);
        let first = SearchEngine::<Doc>::load(dir.path(), emb, tk).unwrap();
        first
            .add(Doc {
                id: "d1".into(),
                title: "first".into(),
                body: "written through the op log only".into(),
            })
            .unwrap();
        first
            .add(Doc {
                id: "d2".into(),
                title: "second".into(),
                body: "same".into(),
            })
            .unwrap();
        drop(first);
        assert!(!dir.path().join("snapshot.json").exists());

        let (tk, emb, _) = engine(32);
        let restored = SearchEngine::<Doc>::load(dir.path(), emb, tk).unwrap();
        assert_eq!(restored.len(), 2);
        assert!(restored.get("d1").is_some());
        assert!(restored.get("d2").is_some());
    }

    #[test]
    fn load_with_missing_root_is_empty_ok() {
        let (tk, emb, _) = engine(32);
        let missing = tempfile::tempdir().unwrap().keep().join("nope");
        // 空引擎语义：目录不存在 → 返回空引擎（Ok）。断言写死这个选择。
        let e = SearchEngine::<Doc>::load(&missing, emb, tk).unwrap();
        assert!(e.is_empty());
    }

    #[test]
    fn compact_rewrites_snapshot_and_truncates_ops() {
        // compact 把 op 尾部折叠进快照：log 清空，load 等价。
        let dir = tempfile::tempdir().unwrap();
        let (_, _, e) = engine(32);
        e.add(Doc {
            id: "d1".into(),
            title: "a".into(),
            body: "x".into(),
        })
        .unwrap();
        e.add(Doc {
            id: "d2".into(),
            title: "b".into(),
            body: "y".into(),
        })
        .unwrap();
        e.save(dir.path()).unwrap();
        e.remove("d1");
        e.add(Doc {
            id: "d3".into(),
            title: "c".into(),
            body: "z".into(),
        })
        .unwrap();
        e.compact();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("ops.jsonl")).unwrap(),
            ""
        );
        let (tk, emb, _) = engine(32);
        let restored = SearchEngine::<Doc>::load(dir.path(), emb, tk).unwrap();
        assert_eq!(restored.len(), 2);
        assert!(restored.get("d1").is_none());
        assert!(restored.get("d2").is_some());
        assert!(restored.get("d3").is_some());
    }

    #[test]
    fn compile_boundary_doc() {
        // SearchEngine<NonSerde> 上不存在 save/load —— 以下在
        // #[cfg(test)] 内用函数签名固化（如果有人把 impl 块的边界去掉，
        // 这个测试仍然编译；真正的边界由类型系统保证，无需额外测试体）。
        type SaveFn<T> =
            fn(&SearchEngine<T>, &std::path::Path) -> crate::error::Result<()>;
        type LoadFn<T> = fn(
            &std::path::Path,
            Arc<dyn Embedder>,
            Arc<dyn Tokenizer>,
        ) -> crate::error::Result<SearchEngine<T>>;

        fn assert_has_save<
            T: Searchable + Serialize + serde::de::DeserializeOwned,
        >(
            _: &SearchEngine<T>,
        ) {
            let _save: SaveFn<T> = SearchEngine::<T>::save;
            let _load: LoadFn<T> = SearchEngine::<T>::load;
        }
        let _ = assert_has_save::<Doc>;
    }
}
