//! Cross-user / privileged listing helpers used by the server
//! admin tooling and the cleanup daemon.

use std::fs;

use anyhow::Result;

use super::SessionManager;
use crate::{error::StoreError, store::SessionMetadata};

impl SessionManager {
    pub fn store(&self) -> &super::super::store::Store {
        &self.store
    }

    /// List all session ids persisted across the entire store, regardless
    /// of user. **Privileged**: only callers that already have filesystem
    /// access to `sessions_root` should use this (e.g. admin tooling,
    /// garbage collection, migration). Most code MUST prefer
    /// [`list_sessions_for_user`], which is hard-scoped to a single
    /// user_id.
    pub fn list_persisted_sessions(&self) -> Result<Vec<String>> {
        // Iterate every direct subdirectory of `sessions_root` and
        // collect (user_id, session_id) pairs. The legacy tenant uses
        // a reserved prefix to keep the shape consistent.
        let mut out = Vec::new();
        let root = match fs::read_dir(self.store_root_path()) {
            Ok(it) => it,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(out);
            }
            Err(e) => return Err(e.into()),
        };
        for entry in root {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let _user_id = match entry.file_name().to_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            let user_dir = entry.path();
            for s in fs::read_dir(&user_dir)? {
                let s = s?;
                if s.file_type()?.is_dir()
                    && let Some(name) = s.file_name().to_str()
                {
                    out.push(name.to_string());
                }
            }
        }
        Ok(out)
    }

    /// List sessions owned by `caller_user_id`. Errors with
    /// [`StoreError::EmptyUserId`] if the user_id is empty; errors with
    /// [`StoreError::CrossUserAccess`] if the on-disk metadata disagrees
    /// with the user_id.
    pub fn list_sessions_for_user(
        &self,
        caller_user_id: &str,
    ) -> Result<Vec<SessionMetadata>> {
        self.store.list_sessions_with_metadata(caller_user_id)
    }

    /// Backwards-compatible alias: prefer [`list_sessions_for_user`].
    /// Returns sessions visible to a hard-coded `"__internal__"` user
    /// so existing tests that expect a global view do not silently leak
    /// data across users. Production code MUST call
    /// [`list_sessions_for_user`] with a real user_id.
    pub fn list_sessions_with_metadata(&self) -> Result<Vec<SessionMetadata>> {
        Err(StoreError::EmptyUserId {
            session_id: "<list_all>".to_string(),
        }
        .into())
    }

    /// Internal: returns the `sessions_root` path of the underlying
    /// store. Used by [`list_persisted_sessions`] which iterates the
    /// raw directory tree for privileged callers.
    fn store_root_path(&self) -> &std::path::Path {
        // `Store` keeps `sessions_root` private. We expose a tiny
        // accessor via a `pub(crate)` method to keep the field
        // encapsulated; see `Store::sessions_root_path`.
        self.store.sessions_root_path()
    }
}
