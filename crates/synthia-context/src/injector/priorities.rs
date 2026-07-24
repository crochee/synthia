/// Default priority constants for context sections.
/// System prompt - highest priority, should never be trimmed.
pub const SYSTEM_PROMPT: u8 = 100;
/// User messages - high priority, preserved unless absolutely necessary.
pub const USER_MESSAGES: u8 = 90;
/// Tool results - medium-high priority.
pub const TOOL_RESULTS: u8 = 70;
/// Injected memories - medium priority.
pub const INJECTED_MEMORIES: u8 = 50;
/// Skill documentation - lower priority, can be trimmed.
pub const SKILL_DOCS: u8 = 40;
/// Workspace info - lowest priority, first to be trimmed.
pub const WORKSPACE_INFO: u8 = 30;
