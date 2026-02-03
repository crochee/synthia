//! Core runtime types for synthia-agent
//!
//! This module contains core event and notification types used during
//! agent execution. These types represent runtime events and status
//! information, NOT persistent data models.
//!
//! ## Type Categories
//!
//! - **Agent Events**: Runtime events emitted during agent execution
//! - **Agent Status**: Current state of an agent instance
//! - **System Notifications**: Messages sent to the user interface
//!
//! For persistent data models (Session, Task, Memory, etc.), see
//! [`crate::storage::types`].

mod event;
mod notification;

pub use event::{AgentEvent, AgentStatus};
pub use notification::{SystemNotification, SystemNotificationType};
