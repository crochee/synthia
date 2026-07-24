//! Private `spill_to_disk` helper used by
//! [`super::truncate_output`] to write the full original
//! content to a per-call file under `cfg.temp_dir` for
//! later retrieval.
//!
//! When both `session_id` and `tool_call_id` are supplied,
//! the file name is deterministic:
//! `<temp_dir>/<session_id>/<tool_call_id>.txt`.
//! Otherwise a `Ulid` is used to avoid collisions.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use ulid::Ulid;

pub(super) fn spill_to_disk(
    content: &str,
    temp_dir: &Path,
    session_id: Option<&str>,
    tool_call_id: Option<&str>,
) -> std::io::Result<PathBuf> {
    fs::create_dir_all(temp_dir)?;

    let path = if let (Some(session_id), Some(tool_call_id)) =
        (session_id, tool_call_id)
    {
        let dir = temp_dir.join(sanitize(session_id));
        fs::create_dir_all(&dir)?;
        dir.join(format!("{}.txt", sanitize(tool_call_id)))
    } else {
        let id = Ulid::new();
        temp_dir.join(format!("synthia-truncate-{id}.txt"))
    };

    let mut f = fs::File::create(&path)?;
    f.write_all(content.as_bytes())?;
    f.sync_all()?;

    // Restrict access to the owner only on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(path)
}

fn sanitize(segment: &str) -> String {
    segment
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c == '\0' {
                '_'
            } else {
                c
            }
        })
        .collect()
}
