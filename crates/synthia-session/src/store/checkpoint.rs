//! `checkpoint_{step}.json` read/write. Each save produces a new
//! file; `load_latest` returns the highest-step checkpoint present
//! in the session directory.

use std::{io::Write, path::Path};

use anyhow::Result;

use super::{dir::ensure_session_dir, types::CheckpointData};

/// Write `checkpoint_{step}.json` atomically under `dir`.
pub(crate) fn save_to(dir: &Path, data: &CheckpointData) -> Result<()> {
    ensure_session_dir(dir)?;
    let path = dir.join(format!("checkpoint_{}.json", data.step));
    let json = serde_json::to_string_pretty(data)?;
    let temp_path = dir.join(format!("checkpoint_{}.tmp", data.step));
    let mut file = std::fs::File::create(&temp_path)?;
    file.write_all(json.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&temp_path, &path)?;
    Ok(())
}

/// Read the highest-step checkpoint under `dir`. Returns `None`
/// if no checkpoints exist or the directory is missing.
pub(crate) fn load_latest_from(dir: &Path) -> Result<Option<CheckpointData>> {
    if !dir.exists() {
        return Ok(None);
    }
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                return None;
            }
            let step = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|stem| stem.strip_prefix("checkpoint_"))
                .and_then(|num| num.parse::<usize>().ok())?;
            Some((step, path))
        })
        .collect();
    if entries.is_empty() {
        return Ok(None);
    }
    entries.sort_by_key(|(step, _)| *step);
    let (_, latest_path) = entries.last().unwrap();
    let content = std::fs::read_to_string(latest_path)?;
    let data: CheckpointData = serde_json::from_str(&content)?;
    Ok(Some(data))
}
