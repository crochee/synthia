//! Stable per-machine identity for the CLI REPL.
//!
//! The CLI is a local interactive prompt; there is no auth context to
//! source a `user_id` from. Without a stable identity, every CLI
//! invocation would create sessions in a single shared namespace
//! (the historical `_legacy_` placeholder), breaking the §1
//! user_id isolation guarantee.
//!
//! Resolution order:
//!
//! 1. `SYNTHIA_USER_ID` environment variable, if set and non-empty.
//!    Lets CI / test environments pin a deterministic id without
//!    touching the filesystem.
//! 2. `~/.synthia/identity` on disk, if it exists and contains a
//!    well-formed 32-char hex string. The file is chmod 0o600 on Unix
//!    so it is not readable by other users on the same machine.
//! 3. A fresh random 32-char hex (16 random bytes), generated and
//!    atomically written (temp file + rename, 0o600) to
//!    `~/.synthia/identity`.
//!
//! The id is generated once per machine, not once per process. Each
//! CLI invocation reads the same file and reuses the same namespace
//! for the rest of the machine's lifetime.
//!
//! ## Why hex and not ULID/UUID
//!
//! - Hex passes every filesystem charset check across OS / FS combinations.
//! - 128 bits is enough entropy to be collision-safe in the
//!   "one per machine" use case (≈ 2^64 machines before 50% collision).
//! - No PII (no timestamp, no machine fingerprint).

use std::{fs, io::Write, path::PathBuf};

use rand::RngCore;

/// Length of the persisted identity in hex characters (32 hex =
/// 128 bits). Treated as the wire format; do not change without a
/// migration.
const IDENTITY_HEX_LEN: usize = 32;

/// Subdirectory under `$HOME` where the identity file lives.
const IDENTITY_DIR: &str = ".synthia";

/// Filename for the identity document.
const IDENTITY_FILE: &str = "identity";

/// Environment variable that overrides the persisted identity. Use
/// this in CI to pin a stable user_id without touching the
/// filesystem.
const IDENTITY_ENV: &str = "SYNTHIA_USER_ID";

/// A stable per-machine CLI user identity.
#[derive(Debug, Clone)]
pub struct Identity {
    user_id: String,
    path: PathBuf,
}

impl Identity {
    /// Load the identity from disk, or generate and persist a fresh
    /// one if it does not exist.
    ///
    /// Behavior:
    /// - If `SYNTHIA_USER_ID` is set, return it directly **without**
    ///   writing to disk. The env var is the "I'm a test / CI" escape
    ///   hatch.
    /// - If the file is missing, generate a new one and persist it.
    /// - If the file is present but corrupted (not 32-char hex), log a
    ///   warning, regenerate, and overwrite.
    /// - If `$HOME` is not set and the env var is not set, return an
    ///   error — the CLI cannot run without an identity root.
    pub fn load_or_create() -> Result<Self, IdentityError> {
        // 1. Environment override.
        if let Ok(env_id) = std::env::var(IDENTITY_ENV)
            && !env_id.is_empty()
        {
            return Ok(Self {
                user_id: env_id,
                path: PathBuf::from("<SYNTHIA_USER_ID>"),
            });
        }

        let path = identity_path()?;
        if let Some(user_id) = Self::read_file(&path)? {
            return Ok(Self { user_id, path });
        }

        // Missing or corrupted → regenerate.
        let user_id = generate_random_id();
        write_file(&path, &user_id)?;
        Ok(Self { user_id, path })
    }

    /// The user_id, e.g. `"a1b2c3d4..."` (32 hex chars).
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// The on-disk path the identity was loaded from, or
    /// `<SYNTHIA_USER_ID>` when the env var is the source.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    fn read_file(path: &PathBuf) -> Result<Option<String>, IdentityError> {
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(path).map_err(|source| {
            IdentityError::ReadFailed {
                path: path.clone(),
                source,
            }
        })?;
        let trimmed = content.trim();
        if !is_valid_hex(trimmed) {
            // Corrupt — caller will regenerate.
            eprintln!(
                "warning: identity file at {} is malformed ({} bytes); regenerating",
                path.display(),
                trimmed.len()
            );
            return Ok(None);
        }
        Ok(Some(trimmed.to_string()))
    }
}

/// Path of the identity file under `$HOME`.
fn identity_path() -> Result<PathBuf, IdentityError> {
    let home = std::env::var("HOME").map_err(|_| IdentityError::NoHome)?;
    Ok(PathBuf::from(home).join(IDENTITY_DIR).join(IDENTITY_FILE))
}

fn generate_random_id() -> String {
    let mut bytes = [0u8; IDENTITY_HEX_LEN / 2];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn is_valid_hex(s: &str) -> bool {
    s.len() == IDENTITY_HEX_LEN
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn write_file(path: &PathBuf, content: &str) -> Result<(), IdentityError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| {
            IdentityError::CreateDirFailed {
                path: parent.to_path_buf(),
                source,
            }
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // 0o700 on the directory: owner-only listing.
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
                .map_err(|source| IdentityError::SetPermissions {
                    path: parent.to_path_buf(),
                    source,
                })?;
        }
    }
    // Atomic write: temp file + rename.
    let temp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&temp).map_err(|source| {
            IdentityError::WriteFailed {
                path: temp.clone(),
                source,
            }
        })?;
        file.write_all(content.as_bytes()).map_err(|source| {
            IdentityError::WriteFailed {
                path: temp.clone(),
                source,
            }
        })?;
        file.sync_all()
            .map_err(|source| IdentityError::WriteFailed {
                path: temp.clone(),
                source,
            })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // 0o600 on the file: owner-only read/write.
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|source| IdentityError::SetPermissions {
                    path: temp.clone(),
                    source,
                })?;
        }
    }
    fs::rename(&temp, path).map_err(|source| IdentityError::RenameFailed {
        from: temp,
        to: path.clone(),
        source,
    })?;
    Ok(())
}

/// Errors that can occur when loading or creating a CLI identity.
#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error(
        "$HOME is not set and SYNTHIA_USER_ID is not set; cannot determine CLI identity root"
    )]
    NoHome,

    #[error("failed to read identity file at {path}: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to create identity directory {path}: {source}")]
    CreateDirFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to set permissions on {path}: {source}")]
    SetPermissions {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write identity file {path}: {source}")]
    WriteFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to rename identity file {from} -> {to}: {source}")]
    RenameFailed {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_valid_hex_accepts_lowercase_32() {
        assert!(is_valid_hex(&"a".repeat(32)));
        assert!(is_valid_hex("0123456789abcdef0123456789abcdef"));
    }

    #[test]
    fn is_valid_hex_rejects_wrong_length() {
        assert!(!is_valid_hex(""));
        assert!(!is_valid_hex("abc"));
        assert!(!is_valid_hex(&"a".repeat(31)));
        assert!(!is_valid_hex(&"a".repeat(33)));
    }

    #[test]
    fn is_valid_hex_rejects_uppercase() {
        // All-uppercase is rejected so the on-disk format is
        // canonical: every CLI on a given machine writes the
        // same encoding.
        assert!(!is_valid_hex(&"A".repeat(32)));
    }

    #[test]
    fn is_valid_hex_rejects_non_hex_chars() {
        assert!(!is_valid_hex("g23456789abcdef0123456789abcdef0"));
        assert!(!is_valid_hex("z23456789abcdef0123456789abcdef0"));
    }

    #[test]
    fn generate_random_id_returns_32_lowercase_hex() {
        let id = generate_random_id();
        assert_eq!(id.len(), 32);
        assert!(is_valid_hex(&id));
    }

    #[test]
    fn generate_random_id_distinguishes_two_calls() {
        // Birthday-bound: 128 bits → collision probability is
        // negligible, but a flaky failure here would be a real
        // bug, not random chance. Re-run on failure before
        // debugging.
        let a = generate_random_id();
        let b = generate_random_id();
        assert_ne!(a, b);
    }

    /// ENV override must take precedence over the on-disk file.
    /// This is the CI / test escape hatch.
    #[test]
    fn env_var_overrides_disk() {
        // SAFETY: tests in this module run serially within a process
        // and the env var is only set/unset by these tests.
        let original = std::env::var(IDENTITY_ENV).ok();
        // Use a value that is impossible to generate randomly
        // (a deterministic, traceable pattern).
        unsafe {
            std::env::set_var(IDENTITY_ENV, "ci-override-user-1234");
        }
        let id = Identity::load_or_create()
            .expect("env override should always succeed");
        assert_eq!(id.user_id(), "ci-override-user-1234");
        // Cleanup
        match original {
            Some(v) => unsafe { std::env::set_var(IDENTITY_ENV, v) },
            None => unsafe { std::env::remove_var(IDENTITY_ENV) },
        }
    }
}
