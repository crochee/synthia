//! Session storage format version.
//!
//! v1: legacy `Session` flat struct + 21-file `store/`.
//! v2: part-based `Message`/`Part`/`ToolState` + append-only `SessionTree`.
//!
//! `SessionHeader.version` is always set to `CURRENT_SESSION_VERSION` on save.

pub const CURRENT_SESSION_VERSION: u32 = 2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_session_version_is_v2() {
        assert_eq!(CURRENT_SESSION_VERSION, 2);
    }
}
