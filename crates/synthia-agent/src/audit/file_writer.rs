//! The [`FileAuditWriter`] struct — an inherent (non-trait)
//! single-entry writer used by code paths that bypass the
//! buffered [`super::logger::AuditLogger`].
//!
//! The `AuditWriter` trait that previously abstracted over
//! backend implementations was removed on 2026-06-15 in change
//! `2026-06-15-p2-trait-cleanup` because it had 0 trait-bound
//! usage, 0 dyn dispatch, and exactly 1 real implementation.

use std::{fs::OpenOptions, io::Write, path::PathBuf};

use synthia_core::Error;

use super::entry::AuditEntry;

/// File-based audit writer.
pub struct FileAuditWriter {
    path: PathBuf,
}

impl FileAuditWriter {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub async fn write(&mut self, entry: &AuditEntry) -> Result<(), Error> {
        let json = serde_json::to_string(entry)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", json)?;
        Ok(())
    }
}
