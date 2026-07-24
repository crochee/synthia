//! Save / load passthrough and incremental-save orchestration.
//!
//! The simple pass-through methods delegate to the underlying
//! [`crate::store::Store`]; the incremental-save methods
//! (`incremental_save`, `save_after_tool_call`, `save_on_shutdown`,
//! `save_on_pause`) also update the `last_saved_offsets` map so
//! downstream code can detect unsaved messages.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::manager::SessionManager;

impl SessionManager {
    pub fn save_metadata(&self, session: &crate::types::Session) -> Result<()> {
        self.store.save_metadata(session)
    }

    pub fn append_message(
        &self,
        session_id: &str,
        message: &impl Serialize,
    ) -> Result<()> {
        let user_id = self.user_id_for(session_id)?;
        self.store.append_message(&user_id, session_id, message)
    }

    pub fn load_messages_recent<T>(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let user_id = self.user_id_for(session_id)?;
        self.store.load_messages_recent(&user_id, session_id, limit)
    }

    pub fn load_messages_all<T>(&self, session_id: &str) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let user_id = self.user_id_for(session_id)?;
        self.store.load_messages_all(&user_id, session_id)
    }

    pub fn delete_session(&self, session_id: &str) -> Result<()> {
        let user_id = self.user_id_for(session_id)?;
        self.store.delete_session(user_id.as_str(), session_id)
    }

    pub async fn incremental_save(&self, session_id: &str) -> Result<()> {
        let user_id = self.user_id_for(session_id)?;
        let count = {
            let all_messages: Vec<serde_json::Value> =
                self.store.load_messages_all(&user_id, session_id)?;
            all_messages.len()
        };
        {
            let mut offsets =
                self.last_saved_offsets.write().expect("RwLock poisoned");
            offsets.insert(session_id.to_string(), count);
        }

        let sessions = self.sessions.read().expect("RwLock poisoned");
        if let Some(session) = sessions.get(session_id) {
            self.save_metadata(session)?;
        }
        Ok(())
    }

    /// Incremental save triggered after a tool call completes.
    /// Updates the save offset and writes metadata to reflect the current state.
    pub async fn save_after_tool_call(&self, session_id: &str) -> Result<()> {
        let user_id = self.user_id_for(session_id)?;
        // Update offset to reflect current message count
        let count = {
            let all_messages: Vec<serde_json::Value> =
                self.store.load_messages_all(&user_id, session_id)?;
            all_messages.len()
        };
        {
            let mut offsets =
                self.last_saved_offsets.write().expect("RwLock poisoned");
            offsets.insert(session_id.to_string(), count);
        }

        // Update metadata if session is in memory
        let sessions = self.sessions.read().expect("RwLock poisoned");
        if let Some(session) = sessions.get(session_id) {
            self.save_metadata(session)?;
        }
        Ok(())
    }

    /// Incremental save triggered on graceful shutdown.
    /// Ensures metadata is written with final state.
    pub async fn save_on_shutdown(&self, session_id: &str) -> Result<()> {
        let user_id = self.user_id_for(session_id)?;
        // Update offset to reflect current message count
        let count = {
            let all_messages: Vec<serde_json::Value> =
                self.store.load_messages_all(&user_id, session_id)?;
            all_messages.len()
        };
        {
            let mut offsets =
                self.last_saved_offsets.write().expect("RwLock poisoned");
            offsets.insert(session_id.to_string(), count);
        }

        // Update metadata with final state
        let sessions = self.sessions.read().expect("RwLock poisoned");
        if let Some(session) = sessions.get(session_id) {
            self.save_metadata(session)?;
        }
        Ok(())
    }

    /// Incremental save triggered when session is paused.
    /// Persists current state for later resumption.
    pub async fn save_on_pause(&self, session_id: &str) -> Result<()> {
        let user_id = self.user_id_for(session_id)?;
        // Update offset to reflect current message count
        let count = {
            let all_messages: Vec<serde_json::Value> =
                self.store.load_messages_all(&user_id, session_id)?;
            all_messages.len()
        };
        {
            let mut offsets =
                self.last_saved_offsets.write().expect("RwLock poisoned");
            offsets.insert(session_id.to_string(), count);
        }

        // Update metadata to reflect paused state
        let sessions = self.sessions.read().expect("RwLock poisoned");
        if let Some(session) = sessions.get(session_id) {
            self.save_metadata(session)?;
        }
        Ok(())
    }

    pub async fn load_messages_paginated<T>(
        &self,
        session_id: &str,
        page: usize,
        page_size: usize,
    ) -> Result<Vec<T>>
    where
        T: for<'de> Deserialize<'de> + Clone,
    {
        let user_id = self.user_id_for(session_id)?;
        let all_messages: Vec<T> =
            self.store.load_messages_all(&user_id, session_id)?;
        let start = page * page_size;
        let end = (start + page_size).min(all_messages.len());

        if start >= all_messages.len() {
            return Ok(Vec::new());
        }

        Ok(all_messages[start..end].to_vec())
    }
}
