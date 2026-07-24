use std::collections::HashMap;

use super::types::{PromptStateSnapshot, compute_hash};

pub fn create_prompt_snapshot(
    system_content: &str,
    tools_content: &str,
    model: &str,
    fast_mode: bool,
    cache_mark: &super::super::mark::CacheControlMark,
) -> PromptStateSnapshot {
    let system_hash = compute_hash(system_content);
    let tools_hash = compute_hash(tools_content);
    let cache_control_hash = cache_mark.hash_to_u64();
    let prefix_hash = compute_hash(system_content);

    PromptStateSnapshot {
        system_content: system_content.to_string(),
        system_hash,
        tools_hash,
        per_tool_hashes: HashMap::new(),
        cache_control_hash,
        model: model.to_string(),
        fast_mode,
        betas: Vec::new(),
        global_cache_strategy: String::new(),
        timestamp: chrono::Utc::now().timestamp(),
        volatile_sections: Vec::new(),
        prefix_hash: format!("{prefix_hash:x}"),
    }
}
