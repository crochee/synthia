//! Regex-based conditional routing.
//!
//! Adopted from traitclaw
//! `crates/traitclaw-team/src/conditional_router.rs`.
//!
//! [`ConditionalRouter`] holds ordered regex rules: the first rule
//! whose pattern matches the text picks the target, and when no
//! rule matches the configured default is used. Matching is the
//! unanchored `is_match` semantics of the `regex` crate, so
//! `search` matches "please search the RFC".
//!
//! It implements the crate's [`Router`] trait, so it can sit in
//! the same slot as [`super::LeaderRouter`]: the selected target
//! becomes one [`Mention`] carrying the whole input text as its
//! prompt. Compiling a bad pattern is a typed
//! [`RoutePatternError`] rather than a panic, so routers can be
//! built from configuration.
//!
//! # Example
//!
//! ```
//! use synthia_tool_task::{ConditionalRouter, Router};
//!
//! let router = ConditionalRouter::new(Some("general".to_string()))
//!     .when("search|find", "researcher")?
//!     .when("write|draft", "writer")?;
//!
//! assert_eq!(router.select("please search the RFC"), Some("researcher"));
//! assert_eq!(router.select("draft a summary"), Some("writer"));
//! assert_eq!(router.select("hello"), Some("general"));
//! # Ok::<(), synthia_tool_task::RoutePatternError>(())
//! ```

use regex::Regex;
use thiserror::Error;

use super::router::{Mention, Router, RoutingDecision};

/// A routing rule whose pattern is not a valid regex.
#[derive(Debug, Error)]
#[error("invalid route pattern `{pattern}`: {source}")]
pub struct RoutePatternError {
    /// The pattern, exactly as supplied to
    /// [`ConditionalRouter::when`].
    pub pattern: String,
    /// The regex compiler's report.
    #[source]
    pub source: regex::Error,
}

/// One compiled rule: text matching `pattern` routes to `target`.
#[derive(Debug)]
struct Rule {
    pattern: Regex,
    target: String,
}

/// Content-based router: the first matching rule wins.
#[derive(Debug)]
pub struct ConditionalRouter {
    rules: Vec<Rule>,
    default: Option<String>,
}

impl ConditionalRouter {
    /// Create a router with `default` as the fallback target.
    ///
    /// With `None`, an input that matches no rule routes nowhere:
    /// [`select`](Self::select) returns `None` and
    /// [`route`](Router::route) returns
    /// [`RoutingDecision::PassThrough`].
    #[must_use]
    pub fn new(default: Option<String>) -> Self {
        Self {
            rules: Vec::new(),
            default,
        }
    }

    /// Append a rule: text matching `pattern` routes to `target`.
    ///
    /// Rules are checked in insertion order and the first match
    /// wins, so add the most specific pattern first. The pattern
    /// is unanchored — a rule fires wherever it matches inside the
    /// text.
    ///
    /// # Errors
    ///
    /// Returns [`RoutePatternError`] when `pattern` does not
    /// compile; the rule is not added.
    pub fn when(
        mut self,
        pattern: &str,
        target: &str,
    ) -> Result<Self, RoutePatternError> {
        let compiled =
            Regex::new(pattern).map_err(|source| RoutePatternError {
                pattern: pattern.to_string(),
                source,
            })?;
        self.rules.push(Rule {
            pattern: compiled,
            target: target.to_string(),
        });
        Ok(self)
    }

    /// The target `text` routes to: the first matching rule's
    /// target, else the default.
    #[must_use]
    pub fn select(&self, text: &str) -> Option<&str> {
        for rule in &self.rules {
            if rule.pattern.is_match(text) {
                return Some(&rule.target);
            }
        }
        self.default.as_deref()
    }
}

impl Router for ConditionalRouter {
    fn name(&self) -> &str {
        "conditional"
    }

    fn description(&self) -> &str {
        "Routes by ordered regex rules; first match wins, else the default."
    }

    fn route(&self, text: &str) -> RoutingDecision {
        match self.select(text) {
            Some(target) => RoutingDecision::Route {
                mentions: vec![Mention {
                    agent: target.to_string(),
                    prompt: text.to_string(),
                    start: 0,
                    end: text.len(),
                }],
                remainder: String::new(),
            },
            None => RoutingDecision::PassThrough,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn the_first_matching_rule_wins() {
        // Both rules match; insertion order decides.
        let router = ConditionalRouter::new(Some("general".to_string()))
            .when("search|find", "researcher")
            .unwrap()
            .when("search", "search_engine")
            .unwrap();

        assert_eq!(router.select("please search the RFC"), Some("researcher"));
    }

    #[test]
    fn a_selected_target_becomes_a_mention_over_the_whole_text() {
        let router = ConditionalRouter::new(None)
            .when("search", "researcher")
            .unwrap();

        let text = "please search the RFC";
        match router.route(text) {
            RoutingDecision::Route {
                mentions,
                remainder,
            } => {
                assert_eq!(mentions.len(), 1);
                assert_eq!(mentions[0].agent, "researcher");
                assert_eq!(mentions[0].prompt, text);
                assert_eq!(mentions[0].start, 0);
                assert_eq!(mentions[0].end, text.len());
                assert!(remainder.is_empty());
            }
            other => panic!("expected a route, got {other:?}"),
        }
    }

    #[test]
    fn unmatched_text_falls_back_to_the_default() {
        let router = ConditionalRouter::new(Some("general".to_string()))
            .when("search", "researcher")
            .unwrap();

        assert_eq!(router.select("hello there"), Some("general"));
        assert_eq!(router.route("hello there").mentions()[0].agent, "general");
    }

    #[test]
    fn no_match_and_no_default_passes_through() {
        let router = ConditionalRouter::new(None)
            .when("search", "researcher")
            .unwrap();

        assert_eq!(router.select("hello there"), None);
        assert_eq!(router.route("hello there"), RoutingDecision::PassThrough);
    }

    #[test]
    fn a_bad_pattern_is_a_typed_error() {
        let error = ConditionalRouter::new(None)
            .when("(", "broken")
            .unwrap_err();

        assert_eq!(error.pattern, "(");
        assert!(error.source().is_some());
    }
}
