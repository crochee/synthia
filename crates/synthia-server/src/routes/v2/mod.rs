//! V2 API REST endpoints with `ApiResponse` envelope.
//!
//! The public route handlers are split into domain modules
//! (one per resource group) plus a private helpers module. List
//! handlers return `Json<PaginatedResponse<T>>`; single-resource
//! handlers return `Json<ApiResponse<T>>`.
//!
//! # Module Layout
//!
//! - [`providers`]: 4 handlers + 6 request/response types.
//! - [`skills`]: 5 handlers + 7 request/response types.
//! - [`memory`]: 1 handler + 3 request/response types.
//! - [`sessions`]: 4 handlers (`create_session_v2`, `list_sessions_v2`,
//!   `get_session_detail`, `delete_session_v2`) + session DTOs.
//! - [`prompts`]: 1 handler (`create_prompt`) + prompt DTOs.
//! - [`steering`]: 1 handler (`create_steering`) + steering DTOs.
//! - [`cancel`]: 1 handler (`cancel_session`) + cancel DTOs.
//! - [`events`]: 1 handler (`session_events`) + event query DTOs.
//! - [`messages`]: 1 handler (`list_messages`) + message DTOs.
//! - [`helpers`]: 1 private helper ([`helpers::copy_dir_all`]).

mod cancel;
mod events;
mod helpers;
mod memory;
mod messages;
mod models;
mod prompts;
mod providers;
mod sessions;
mod skills;
mod steering;
mod subagents;

pub use cancel::cancel_session;
pub use events::session_events;
pub use memory::{
    MemoryResult,
    MemorySearchQuery,
    MemorySearchResponse,
    search_memory,
};
pub use messages::list_messages;
pub use models::{
    CancelRequest,
    CancelResponse,
    CreateSessionRequest,
    EventsQuery,
    MessageCursor,
    MessageItem,
    MessagesQuery,
    PromptAcceptedResponse,
    PromptRequest,
    SessionListCursor,
    SessionListQuery,
    SessionResponse,
    SessionSummaryResponse,
    SteeringAcceptedResponse,
    SteeringRequest,
};
pub use prompts::create_prompt;
pub use providers::{
    CreateProviderRequest,
    ProviderCreatedResponse,
    ProviderDeletedResponse,
    ProviderDetailResponse,
    ProviderInfo,
    ProviderListResponse,
    create_provider,
    delete_provider,
    get_provider,
    list_providers,
};
pub use sessions::{
    create_session_v2,
    delete_session_v2,
    get_session_detail,
    list_sessions_v2,
};
pub use skills::{
    CreateSkillRequest,
    SkillCreatedResponse,
    SkillDeletedResponse,
    SkillDetailResponse,
    SkillInfo,
    SkillListResponse,
    SkillReloadResponse,
    create_skill,
    delete_skill,
    get_skill,
    list_skills,
    reload_skills,
};
pub use steering::create_steering;
pub use subagents::list_subagents;
