//! History management for REPL

use std::path::PathBuf;

/// Load and save REPL history from a file path.
pub fn load_history(
    path: &PathBuf,
    rl: &mut rustyline::DefaultEditor,
) -> anyhow::Result<()> {
    if path.exists() {
        rl.load_history(path)?;
    }
    Ok(())
}

/// Save REPL history to a file path.
pub fn save_history(
    path: &PathBuf,
    rl: &mut rustyline::DefaultEditor,
) -> anyhow::Result<()> {
    rl.save_history(path)?;
    Ok(())
}
