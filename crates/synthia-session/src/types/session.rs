//! The [`Session`] struct — the state-machine model record.
//!
//! Three constructors are exposed:
//!
//! - [`Session::new`] — legacy single-tenant; leaves
//!   `user_id` empty. Persistence of an empty-`user_id`
//!   session is rejected by `Store::save_metadata` /
//!   `Store::ensure_session_dir`, so this path is in-memory
//!   only until the caller calls [`Session::assign_user`].
//! - [`Session::new_with_user`] — preferred multi-tenant
//!   constructor. Returns `Err(StoreError::EmptyUserId)` if
//!   `user_id` is empty, matching the project memory hard
//!   constraint that every persisted session MUST be
//!   namespaced by `user_id` (no global / anonymous sessions).
//! - [`Session::with_config`] — bypasses the default
//!   `SessionConfig` / `TokenBudget` (used by
//!   `Session::new` / `Session::new_with_user` internally).

use chrono::{DateTime, Utc};

use super::{
    config::SessionConfig,
    state::InvalidStateTransition,
    token_budget::{TokenBudget, TokenBudgetStatus},
};
use crate::{error::StoreError, types::TokenUsage};

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    /// Owning user identifier. Used by `Store` to namespace session paths
    /// under `{sessions_root}/{user_id}/{session_id}/`. The empty string is
    /// reserved for the legacy single-tenant layout and is rejected by
    /// `Store::ensure_session_dir` and `Session::new_with_user` once the
    /// caller opts into multi-tenant mode.
    pub user_id: String,
    pub state: super::state::SessionState,
    pub token_usage: TokenUsage,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub config: SessionConfig,
    pub needs_save: bool,
    pub token_budget: TokenBudget,
    pub context_window: usize,
    pub end_reason: Option<String>,
    pub iteration: usize,
    pub cumulative_tokens: usize,
    pub context_token_limit: Option<usize>,
    /// Optional identifier of the parent session. Set when this
    /// session is spawned as a child (subagent) of another session.
    pub parent_id: Option<String>,
}

impl Session {
    /// Legacy single-tenant constructor: leaves `user_id` empty.
    ///
    /// Persistence of an empty-`user_id` session is rejected by
    /// `Store::save_metadata` / `Store::ensure_session_dir`, so any session
    /// constructed via this path is in-memory only until the caller assigns
    /// a real `user_id` via `Session::assign_user` (or rebuilds it through
    /// `Session::new_with_user`). New code MUST prefer
    /// [`Session::new_with_user`].
    pub fn new(id: String) -> Self {
        let mut s = Self::with_config(
            id,
            SessionConfig::default(),
            TokenBudget::default(),
        );
        s.user_id = String::new();
        s
    }

    /// Assign or replace the `user_id` on an in-memory session.
    pub fn assign_user(&mut self, user_id: String) {
        self.user_id = user_id;
    }

    /// Multi-tenant constructor. Returns `Err(StoreError::EmptyUserId)` if
    /// `user_id` is empty, matching the project memory hard constraint that
    /// every persisted session MUST be namespaced by `user_id` (no global /
    /// anonymous sessions).
    pub fn new_with_user(
        id: String,
        user_id: String,
    ) -> Result<Self, StoreError> {
        if user_id.is_empty() {
            return Err(StoreError::EmptyUserId { session_id: id });
        }
        let mut s = Self::with_config(
            id,
            SessionConfig::default(),
            TokenBudget::default(),
        );
        s.user_id = user_id;
        Ok(s)
    }

    pub fn with_config(
        id: String,
        config: SessionConfig,
        budget: TokenBudget,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            user_id: String::new(),
            state: super::state::SessionState::Initializing,
            token_usage: TokenUsage::default(),
            created_at: now,
            updated_at: now,
            config,
            needs_save: false,
            token_budget: budget,
            context_window: 128_000,
            end_reason: None,
            iteration: 0,
            cumulative_tokens: 0,
            context_token_limit: None,
            parent_id: None,
        }
    }

    pub fn budget_status(&self) -> TokenBudgetStatus {
        self.token_budget.check(self.token_usage.total_tokens)
    }

    pub fn needs_pre_sampling_compact(&self) -> bool {
        self.budget_status() == TokenBudgetStatus::MustCompact
    }

    pub fn needs_mid_turn_compact(&self) -> bool {
        self.budget_status() == TokenBudgetStatus::MustCompact
    }

    pub fn context_safety_check(&self) -> Result<(), &'static str> {
        let available = self
            .context_window
            .saturating_sub(self.token_usage.total_tokens);
        if available < CONTEXT_HARD_MIN {
            return Err("Context window below hard minimum (16K tokens)");
        }
        if available < CONTEXT_WARN_BELOW {
            return Ok(());
        }
        Ok(())
    }

    pub fn context_available(&self) -> usize {
        self.context_window
            .saturating_sub(self.token_usage.total_tokens)
    }

    pub fn record_token_usage(&mut self, usage: TokenUsage) {
        self.token_usage = usage;
        self.updated_at = Utc::now();
        self.needs_save = true;
    }

    pub fn add_token_usage(
        &mut self,
        prompt: usize,
        completion: usize,
        cached: Option<usize>,
    ) {
        self.token_usage.prompt_tokens += prompt;
        self.token_usage.completion_tokens += completion;
        self.token_usage.total_tokens += prompt + completion;
        if let Some(c) = cached {
            self.token_usage.cached_prompt_tokens =
                Some(self.token_usage.cached_prompt_tokens.unwrap_or(0) + c);
        }
        self.updated_at = Utc::now();
        self.needs_save = true;
    }

    pub fn transition_to(
        &mut self,
        new_state: super::state::SessionState,
    ) -> Result<(), InvalidStateTransition> {
        let old_state = self.state;
        if !Self::is_valid_transition(old_state, new_state) {
            return Err(InvalidStateTransition {
                from: old_state,
                to: new_state,
            });
        }
        self.state = new_state;
        self.updated_at = Utc::now();
        self.needs_save = true;
        Ok(())
    }

    pub fn is_valid_transition(
        from: super::state::SessionState,
        to: super::state::SessionState,
    ) -> bool {
        crate::state_machine::is_valid_transition(from, to)
    }
}

use super::token_budget::{CONTEXT_HARD_MIN, CONTEXT_WARN_BELOW};
