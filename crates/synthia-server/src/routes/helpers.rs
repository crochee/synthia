//! Private helper functions.
//!
//! Currently only [`copy_dir_all`] is exposed, used by
//! [`skills::create_skill`] to recursively install
//! a skill directory into the workspace skills folder.

/// Recursively copy `src` to `dst`. Returns the underlying
/// I/O error on failure.
pub(super) fn copy_dir_all(
    src: &std::path::Path,
    dst: &std::path::Path,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
