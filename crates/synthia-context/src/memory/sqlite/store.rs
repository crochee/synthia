//! [`SqliteMemory`] — constructors, pragmas, and the long-term
//! read/write path.

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags, params};
use ulid::Ulid;

use super::{
    decode::{read_entry, rfc3339},
    error::{SqliteMemoryError, query_err},
    fts::fts_query,
    schema::{SCHEMA, has_schema},
};
use crate::memory::{InMemoryMemory, Memory, MemoryEntry};
/// `SQLite`-backed memory: durable long-term, working, and session
/// state in one file.
///
/// The conversation tier delegates to `inner` (see the module docs).
/// Cloning is not supported: the connection is shared behind the
/// type's own lock, so share a [`SqliteMemory`] through an `Arc`
/// instead.
pub struct SqliteMemory {
    pub(super) conn: Mutex<Connection>,
    pub(super) inner: Arc<dyn Memory>,
    read_only: bool,
    /// `None` for an in-memory database.
    pub(super) path: Option<PathBuf>,
    /// Wall-clock source for the timestamps this store writes
    /// (`sessions.created_at`). Default
    /// [`synthia_core::SystemClock`]; injectable via
    /// [`SqliteMemory::with_clock`].
    pub(super) clock: synthia_core::SharedClock,
}

impl fmt::Debug for SqliteMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqliteMemory")
            .field("path", &self.path)
            .field("read_only", &self.read_only)
            .finish_non_exhaustive()
    }
}

impl SqliteMemory {
    /// Open (creating if necessary) the database at `path`, with an
    /// [`InMemoryMemory`] backing the conversation tier.
    ///
    /// # Errors
    ///
    /// [`SqliteMemoryError::Open`] when the file cannot be opened or
    /// created, [`SqliteMemoryError::Migrate`] when the schema cannot
    /// be applied.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SqliteMemoryError> {
        Self::with_inner(path, Arc::new(InMemoryMemory::new()))
    }

    /// Open a database that exists entirely in memory — the test and
    /// ephemeral-run constructor. `SQLite` reports the same schema
    /// behaviour as a file backend, minus durability.
    ///
    /// # Errors
    ///
    /// [`SqliteMemoryError::Open`] / [`SqliteMemoryError::Migrate`].
    pub fn in_memory() -> Result<Self, SqliteMemoryError> {
        Self::in_memory_with_inner(Arc::new(InMemoryMemory::new()))
    }

    /// [`SqliteMemory::open`] with an explicit conversation-tier
    /// backend (a sink-backed [`Memory`] in production).
    ///
    /// # Errors
    ///
    /// [`SqliteMemoryError::Open`] / [`SqliteMemoryError::Migrate`].
    pub fn with_inner(
        path: impl AsRef<Path>,
        inner: Arc<dyn Memory>,
    ) -> Result<Self, SqliteMemoryError> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path).map_err(|error| {
            SqliteMemoryError::Open {
                path: path.display().to_string(),
                reason: error.to_string(),
            }
        })?;
        Self::finish(conn, inner, false, Some(path))
    }

    /// [`SqliteMemory::in_memory`] with an explicit conversation-tier
    /// backend.
    ///
    /// # Errors
    ///
    /// [`SqliteMemoryError::Open`] / [`SqliteMemoryError::Migrate`].
    pub fn in_memory_with_inner(
        inner: Arc<dyn Memory>,
    ) -> Result<Self, SqliteMemoryError> {
        let conn = Connection::open_in_memory().map_err(|error| {
            SqliteMemoryError::Open {
                path: ":memory:".to_string(),
                reason: error.to_string(),
            }
        })?;
        Self::finish(conn, inner, false, None)
    }

    /// Open an existing database read-only: recall, working-memory
    /// reads, and session listing work; every write is refused with
    /// [`SqliteMemoryError::ReadOnly`].
    ///
    /// # Errors
    ///
    /// [`SqliteMemoryError::Open`], or
    /// [`SqliteMemoryError::MissingSchema`] when the file has no
    /// schema to read — a read-only open cannot create one.
    pub fn open_read_only(
        path: impl AsRef<Path>,
    ) -> Result<Self, SqliteMemoryError> {
        Self::open_read_only_with_inner(path, Arc::new(InMemoryMemory::new()))
    }

    /// [`SqliteMemory::open_read_only`] with an explicit
    /// conversation-tier backend.
    ///
    /// # Errors
    ///
    /// [`SqliteMemoryError::Open`] /
    /// [`SqliteMemoryError::MissingSchema`].
    pub fn open_read_only_with_inner(
        path: impl AsRef<Path>,
        inner: Arc<dyn Memory>,
    ) -> Result<Self, SqliteMemoryError> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(|error| SqliteMemoryError::Open {
            path: path.display().to_string(),
            reason: error.to_string(),
        })?;
        conn.pragma_update(None, "busy_timeout", 5_000)
            .map_err(|error| query_err("set busy_timeout", error))?;
        if !has_schema(&conn)? {
            return Err(SqliteMemoryError::MissingSchema {
                path: path.display().to_string(),
            });
        }
        Ok(Self {
            conn: Mutex::new(conn),
            inner,
            read_only: true,
            path: Some(path),
            clock: synthia_core::SharedClock::system(),
        })
    }

    /// Apply pragmas + schema and wrap the connection.
    fn finish(
        conn: Connection,
        inner: Arc<dyn Memory>,
        read_only: bool,
        path: Option<PathBuf>,
    ) -> Result<Self, SqliteMemoryError> {
        conn.pragma_update(None, "busy_timeout", 5_000)
            .map_err(|error| query_err("set busy_timeout", error))?;
        if path.is_some() {
            // WAL keeps a reader from blocking the writer; in-memory
            // databases have no journal mode to set.
            conn.pragma_update(None, "journal_mode", "WAL")
                .map_err(|error| query_err("set journal_mode", error))?;
        }
        conn.execute_batch(SCHEMA).map_err(|error| {
            SqliteMemoryError::Migrate {
                reason: error.to_string(),
            }
        })?;
        Ok(Self {
            conn: Mutex::new(conn),
            inner,
            read_only,
            path,
            clock: synthia_core::SharedClock::system(),
        })
    }

    /// Install the wall-clock source this store writes
    /// `sessions.created_at` from.
    ///
    /// Tests pass a [`synthia_core::FixedClock`] so a created session
    /// has a deterministic timestamp; a deployment that keeps its own
    /// clock (a broker's, a host's monotonic-plus-offset) passes that.
    #[must_use]
    pub fn with_clock(mut self, clock: synthia_core::SharedClock) -> Self {
        self.clock = clock;
        self
    }

    /// The database file, or `None` for an in-memory database.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether writes are refused.
    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Search long-term memory, best match first.
    ///
    /// # Errors
    ///
    /// [`SqliteMemoryError::Query`] when the statement fails,
    /// [`SqliteMemoryError::Decode`] for an unreadable row.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, SqliteMemoryError> {
        let conn = self.conn.lock();
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut out = Vec::new();
        match fts_query(query) {
            Some(expression) => {
                let mut stmt = conn
                    .prepare(
                        "SELECT m.id, m.content, m.metadata, m.created_at \
                         FROM long_term_memory_fts \
                         JOIN long_term_memory m \
                           ON m.rowid = long_term_memory_fts.rowid \
                         WHERE long_term_memory_fts MATCH ?1 \
                         ORDER BY bm25(long_term_memory_fts), \
                                  m.created_at DESC, m.id DESC \
                         LIMIT ?2",
                    )
                    .map_err(|e| query_err("prepare recall", e))?;
                let rows = stmt
                    .query_map(params![expression, limit], read_entry)
                    .map_err(|e| query_err("recall", e))?;
                for row in rows {
                    out.push(
                        row.map_err(|e| query_err("recall", e))?.map_err(
                            |e| SqliteMemoryError::Decode { reason: e },
                        )?,
                    );
                }
            }
            None => {
                let mut stmt = conn
                    .prepare(
                        "SELECT id, content, metadata, created_at \
                         FROM long_term_memory \
                         ORDER BY created_at DESC, id DESC LIMIT ?1",
                    )
                    .map_err(|e| query_err("prepare recall", e))?;
                let rows = stmt
                    .query_map(params![limit], read_entry)
                    .map_err(|e| query_err("recall", e))?;
                for row in rows {
                    out.push(
                        row.map_err(|e| query_err("recall", e))?.map_err(
                            |e| SqliteMemoryError::Decode { reason: e },
                        )?,
                    );
                }
            }
        }
        Ok(out)
    }

    /// Store (or replace, when the id repeats) one entry.
    ///
    /// An empty or non-`ULID` id gets a fresh `ULID`, matching
    /// [`FileMemory`](crate::memory::FileMemory). Re-storing an id replaces its
    /// content and metadata but **keeps the original `created_at`**:
    /// the row's identity is the id, and a correction should not move
    /// the entry in recency order.
    ///
    /// # Errors
    ///
    /// [`SqliteMemoryError::ReadOnly`] on a read-only database,
    /// [`SqliteMemoryError::Query`] when the write fails.
    pub fn store_entry(
        &self,
        entry: &MemoryEntry,
    ) -> Result<(), SqliteMemoryError> {
        self.writable()?;
        let id = if Ulid::from_string(&entry.id).is_ok() {
            entry.id.clone()
        } else {
            Ulid::generate().to_string()
        };
        let metadata = entry.metadata.as_ref().map(|value| value.to_string());
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO long_term_memory (id, content, metadata, created_at) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(id) DO UPDATE SET \
                content = excluded.content, \
                metadata = excluded.metadata",
            params![id, entry.content, metadata, rfc3339(entry.created_at)],
        )
        .map_err(|e| query_err("store", e))?;
        Ok(())
    }

    /// Refuse a write on a read-only database.
    pub(super) fn writable(&self) -> Result<(), SqliteMemoryError> {
        if self.read_only {
            return Err(SqliteMemoryError::ReadOnly);
        }
        Ok(())
    }
}
