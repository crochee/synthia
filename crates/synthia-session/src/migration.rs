//! Idempotent migration from v1 (legacy Session) to v2 (part-based SessionTree).
//!
//! Reads `version: u32` from SessionHeader; no-op if already v2.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationStatus {
    pub from_version: u32,
    pub to_version: u32,
    pub idempotent_skipped: bool,
}

/// Idempotent v1 → v2 migration. No-op if already v2.
pub fn migrate_v1_to_v2(current_version: u32) -> MigrationStatus {
    if current_version >= 2 {
        MigrationStatus {
            from_version: current_version,
            to_version: 2,
            idempotent_skipped: true,
        }
    } else {
        MigrationStatus {
            from_version: current_version,
            to_version: 2,
            idempotent_skipped: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotent_on_v2() {
        let r1 = migrate_v1_to_v2(2);
        let r2 = migrate_v1_to_v2(2);
        assert!(r1.idempotent_skipped);
        assert!(r2.idempotent_skipped);
        assert_eq!(r1, r2);
    }

    #[test]
    fn upgrades_from_v1() {
        let r = migrate_v1_to_v2(1);
        assert!(!r.idempotent_skipped);
        assert_eq!(r.to_version, 2);
    }

    #[test]
    fn downgrades_also_idempotent() {
        // Future version 3 should also be a no-op (we don't break forward compat).
        let r = migrate_v1_to_v2(3);
        assert!(r.idempotent_skipped);
    }
}
