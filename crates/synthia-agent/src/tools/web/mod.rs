//! Web tools module
//!
//! This module provides web search and fetch tools.

mod fetch;
mod web_search;

pub use fetch::WebFetchTool;
pub use web_search::WebSearchTool;
