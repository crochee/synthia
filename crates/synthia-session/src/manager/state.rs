//! State-machine integration and `Registry<SessionInfo>` trait
//! implementation. The `update_session_state` method bridges
//! between the in-memory `SessionStateMachine` and the
//! approval-timer side effects.

use anyhow::Result;
use async_trait::async_trait;
use synthia_core::{Error, registry::Registry};

use super::{SessionManager, types::SessionFilter};
use crate::{
    state_machine::{StateEnterEffect, StateMachineError},
    types::{InvalidStateTransition, Session, SessionState},
};

impl SessionManager {
    pub async fn update_session_state(
        &self,
        session_id: &str,
        new_state: SessionState,
    ) -> Result<(), InvalidStateTransition> {
        let effect = {
            let mut sessions = self.sessions.write().expect("RwLock poisoned");
            let session =
                sessions.get_mut(session_id).ok_or(InvalidStateTransition {
                    from: SessionState::Initializing,
                    to: new_state,
                })?;

            let mut state_machines =
                self.state_machines.write().expect("RwLock poisoned");
            let sm = state_machines.get_mut(session_id).ok_or(
                InvalidStateTransition {
                    from: SessionState::Initializing,
                    to: new_state,
                },
            )?;

            sm.transition_to(new_state, session).map_err(|e| match e {
                StateMachineError::InvalidTransition(e) => e,
                StateMachineError::Persistence(_) => InvalidStateTransition {
                    from: sm.current_state(),
                    to: new_state,
                },
            })?
        };

        // Handle side effects
        match effect {
            StateEnterEffect::StartApprovalTimeout => {
                self.start_approval_timer(session_id).await;
            }
            StateEnterEffect::CancelApprovalTimeout => {
                self.cancel_approval_timer(session_id).await;
            }
            StateEnterEffect::None => {}
        }

        Ok(())
    }
}

fn lock_err() -> Error {
    Error::Internal("RwLock poisoned".to_string())
}

#[async_trait]
impl Registry<super::types::SessionInfo> for SessionManager {
    type Filter = SessionFilter;

    async fn register(
        &self,
        item: super::types::SessionInfo,
    ) -> Result<super::types::SessionInfo, Error> {
        let id = item.id.clone();
        {
            let sessions = self.sessions.read().map_err(|_| lock_err())?;
            if sessions.contains_key(&id) {
                return Err(Error::AlreadyExists(id));
            }
        }
        // The legacy trait registry does not carry a `user_id`, so we
        // build a session with an empty user_id. The store will refuse
        // to persist it until `assign_user` is called; the
        // `SessionInfo` produced here is informational only.
        let session = Session::new(id.clone());
        {
            let mut sessions = self.sessions.write().map_err(|_| lock_err())?;
            sessions.insert(id, session);
        }
        Ok(item)
    }

    async fn unregister(&self, name: &str) -> Result<(), Error> {
        let removed = self.remove(name).await;
        match removed {
            Some(_) => Ok(()),
            None => Err(Error::NotFound(name.to_string())),
        }
    }

    async fn get(
        &self,
        name: &str,
    ) -> Result<Option<super::types::SessionInfo>, Error> {
        let sessions = self.sessions.read().map_err(|_| lock_err())?;
        Ok(sessions.get(name).map(|s| super::types::SessionInfo {
            id: s.id.clone(),
            name: s.id.clone(),
            description: format!("Session in state {:?}", s.state),
            state: s.state,
            created_at: s.created_at,
        }))
    }

    async fn list(
        &self,
        filter: Option<Self::Filter>,
    ) -> Result<Vec<super::types::SessionInfo>, Error> {
        let sessions = self.sessions.read().map_err(|_| lock_err())?;
        let infos: Vec<super::types::SessionInfo> = sessions
            .values()
            .map(|s| super::types::SessionInfo {
                id: s.id.clone(),
                name: s.id.clone(),
                description: format!("Session in state {:?}", s.state),
                state: s.state,
                created_at: s.created_at,
            })
            .collect();
        match filter {
            Some(f) => {
                Ok(infos.into_iter().filter(|i| f.matches_session(i)).collect())
            }
            None => Ok(infos),
        }
    }
}
