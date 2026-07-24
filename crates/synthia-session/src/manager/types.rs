//! `SessionManager` data shapes: filter, info, message cache entry.
//!
//! These are the public value types and an internal cache entry
//! shared by the manager's sub-modules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use synthia_core::registry::RegistryItem;

use crate::types::SessionState;

pub(super) const MAX_CACHED_SESSIONS: usize = 10;

#[derive(Debug, Clone)]
pub(super) struct CachedMessages {
    pub(super) messages: Vec<serde_json::Value>,
    pub(super) access_order: usize,
}

#[derive(Clone, Debug, Default)]
pub struct SessionFilter {
    pub state: Option<SessionState>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub parent_id: Option<String>,
}

impl SessionFilter {
    pub fn matches_session(&self, info: &SessionInfo) -> bool {
        if let Some(ref state) = self.state
            && info.state != *state
        {
            return false;
        }
        if let Some(ref after) = self.created_after
            && info.created_at < *after
        {
            return false;
        }
        if let Some(ref before) = self.created_before
            && info.created_at > *before
        {
            return false;
        }
        true
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub state: SessionState,
    pub created_at: DateTime<Utc>,
}

/// Lightweight summary of an in-memory session, suitable for listing
/// a user's active sessions without loading the full on-disk record.
#[derive(Clone, Debug)]
pub struct SessionSummary {
    pub id: String,
    pub state: SessionState,
    pub title: String,
    pub updated_at: DateTime<Utc>,
    pub parent_id: Option<String>,
}

impl RegistryItem for SessionInfo {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }
}
