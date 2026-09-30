//! [`LeaderRouter`] — `@agent:` text-routed delegation.
//!
//! Adopted from traitclaw
//! `crates/traitclaw-team/src/router.rs::LeaderRouter` (the
//! `@agent:` delegation syntax) + pi-subagents
//! `src/mention.ts` (the `@handle` mention grammar).
//!
//! ## What it is
//!
//! A [`LeaderRouter`] parses the leader agent's text output for
//! `@<agent_name>: <prompt>` mentions and dispatches each mention
//! to the named peer agent. The result of every mention comes back
//! as a `MentionResult`; the leader then composes them into its
//! next assistant turn.
//!
//! ## When to use
//!
//! Use [`LeaderRouter`] when you want the leader to **encode
//! routing decisions in its own text** rather than calling a
//! separate `task` tool. This is a higher-trust UX for short,
//! deterministic delegation flows:
//!
//! ```text
//! Leader: "Let me ask the researcher about that.
//!          @researcher: find the latest RFC on tier caching."
//! ```
//!
//! Versus the lower-trust `task(agent, prompt)` tool call, which
//! is what the existing delegation.rs seam handles. Both are valid;
//! pick by ergonomics.
//!
//! ## Grammar
//!
//! ```text
//! @<agent_name>: <prompt until end of line or next mention>
//! ```
//!
//! The trigger prefix is `@` (configurable); the colon is
//! required; the prompt runs to the next `\n` or the next `@`. If
//! the agent name does not resolve to a registered peer, the
//! mention is left in the text verbatim (the leader can rephrase).
//!
//! ## Object-safe enough
//!
//! The router is `Send + Sync + 'static` so it can sit in an
//! `Arc<dyn Router>` slot. A run with no router wired behaves as
//! [`PassThroughRouter`]: the text reaches the leader unchanged.

use serde::{Deserialize, Serialize};
use synthia_core::registry::RegistryItem;

/// Default trigger character for mentions. Matches traitclaw + pi-subagents.
pub const DEFAULT_TRIGGER: char = '@';

/// The colon separator between the mention and the prompt.
pub const MENTION_SEPARATOR: char = ':';

/// One parsed mention extracted from the leader's text output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mention {
    /// Agent name (without the `@` trigger).
    pub agent: String,
    /// Prompt text after the `:` separator, trimmed of leading
    /// whitespace. May span multiple lines if the leader emits
    /// explicit `\n` continuations.
    pub prompt: String,
    /// Byte offset of the mention start in the source text.
    pub start: usize,
    /// Byte offset of the mention end (after the prompt).
    pub end: usize,
}

/// Router decision. The router inspects the leader's text and
/// returns either a route (with parsed mentions + remainder) or
/// a pass-through (no mentions found).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutingDecision {
    /// Text contains at least one mention. The router has split it
    /// into the raw mentions + the un-routed remainder.
    Route {
        /// Mentions in left-to-right order.
        mentions: Vec<Mention>,
        /// Source text minus the mention runs (preserves ordering).
        remainder: String,
    },
    /// No mentions. Pass the text through unchanged.
    PassThrough,
}

impl RoutingDecision {
    /// True when at least one mention was parsed.
    pub fn has_mentions(&self) -> bool {
        matches!(self, Self::Route { mentions, .. } if !mentions.is_empty())
    }

    /// Mentions if routed, empty if pass-through.
    pub fn mentions(&self) -> &[Mention] {
        match self {
            Self::Route { mentions, .. } => mentions,
            Self::PassThrough => &[],
        }
    }
}

/// Router trait. Object-safe. `route(text)` is sync because the
/// leader's text output is already in hand when the loop reaches
/// the route boundary; the actual delegation work happens
/// asynchronously elsewhere.
pub trait Router: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn route(&self, text: &str) -> RoutingDecision;
}

/// Pass-through router: never parses mentions. Use this when the
/// leader has no peer agents configured (the `@agent:` syntax
/// would never resolve).
pub struct PassThroughRouter;

impl Router for PassThroughRouter {
    fn name(&self) -> &str {
        "pass-through"
    }

    fn description(&self) -> &str {
        "No routing. Always returns PassThrough."
    }

    fn route(&self, _text: &str) -> RoutingDecision {
        RoutingDecision::PassThrough
    }
}

/// Leader router: parses `@<agent>: <prompt>` mentions out of
/// the leader's text. Trigger character is configurable
/// ([`DEFAULT_TRIGGER`] by default). Unknown agent names are
/// kept in the remainder so the leader can rephrase.
///
/// ## `mention_clone_mode`
///
/// When `false` (default), the leader's model emits a `task`
/// tool call to dispatch each mention. When `true`, mentions
/// spawn through [`crate::mention_clone::MentionClone`] instead — the
/// leader's transcript shows no model reasoning and no
/// tool_use block for the dispatch. Caller-side wiring only:
/// the router decides which mentions go through the clone;
/// the clone's lifecycle is the caller's responsibility.
pub struct LeaderRouter {
    trigger: char,
    /// Known peer names. A mention whose agent name is not in this
    /// set stays in the remainder (verbatim, un-routed) so the
    /// leader can decide to retry or skip.
    peers: Vec<String>,
    /// `true` ⇒ mentions route through [`MentionClone`]
    /// instead of the leader's `task` tool. Default `false`.
    mention_clone_mode: bool,
}

impl LeaderRouter {
    /// Build a leader router for `peers`. The trigger defaults to
    /// `@`; override via [`Self::with_trigger`].
    pub fn new(peers: impl IntoIterator<Item = String>) -> Self {
        Self {
            trigger: DEFAULT_TRIGGER,
            peers: peers.into_iter().collect(),
            mention_clone_mode: false,
        }
    }

    /// Override the trigger character (default `@`).
    #[must_use]
    pub fn with_trigger(mut self, trigger: char) -> Self {
        self.trigger = trigger;
        self
    }

    /// Enable / disable [`MentionClone`](crate::mention_clone::MentionClone)
    /// routing. When `true`, mentions resolve to
    /// `MentionClone` invocations; when `false` (default),
    /// they resolve to the leader's `task` tool call.
    #[must_use]
    pub fn with_mention_clone_mode(mut self, enabled: bool) -> Self {
        self.mention_clone_mode = enabled;
        self
    }

    /// True when [`MentionClone`](crate::mention_clone::MentionClone)
    /// routing is active. Pure getter so the caller can
    /// inspect the configured mode without re-deriving it.
    #[must_use]
    pub fn mention_clone_mode(&self) -> bool {
        self.mention_clone_mode
    }

    /// True when `agent` is a known peer.
    pub fn knows(&self, agent: &str) -> bool {
        self.peers.iter().any(|p| p == agent)
    }
}

/// Convert a [`RoutingDecision`] into the [`TaskSpec`](crate::task::TaskSpec)s the
/// existing delegation seam (`task::runner::run_subagent`)
/// understands. This is the bridge between text-routed mentions
/// (`@agent: prompt`) and the tool-call delegation path: the
/// leader's text mentions become `task` tool calls without the
/// model having to emit an actual tool_use block.
///
/// Returns an empty `Vec` for [`RoutingDecision::PassThrough`].
/// Unknown peers are already filtered out by
/// [`LeaderRouter::route`], so every spec here names a peer the
/// router knows.
pub fn mentions_to_task_specs(
    decision: &RoutingDecision,
) -> Vec<crate::task::TaskSpec> {
    decision
        .mentions()
        .iter()
        .map(|m| crate::task::TaskSpec {
            agent: m.agent.clone(),
            prompt: m.prompt.clone(),
            ..Default::default()
        })
        .collect()
}

impl Router for LeaderRouter {
    fn name(&self) -> &str {
        "leader"
    }

    fn description(&self) -> &str {
        "Parses @agent: prompt mentions out of leader text."
    }

    fn route(&self, text: &str) -> RoutingDecision {
        let mentions = parse_mentions(text, self.trigger);
        if mentions.is_empty() {
            return RoutingDecision::PassThrough;
        }
        // Drop mention runs that resolve to unknown agents from the
        // routing decision; the leader still sees the text, so the
        // leader can decide to rephrase.
        let resolved: Vec<Mention> = mentions
            .into_iter()
            .filter(|m| self.knows(&m.agent))
            .collect();
        if resolved.is_empty() {
            return RoutingDecision::PassThrough;
        }
        let remainder = strip_mentions(text, &resolved);
        RoutingDecision::Route {
            mentions: resolved,
            remainder,
        }
    }
}

impl RegistryItem for LeaderRouter {
    fn name(&self) -> &str {
        Router::name(self)
    }

    fn description(&self) -> &str {
        Router::description(self)
    }
}

/// Parse every `@<name>: <prompt>` mention out of `text`. The
/// prompt runs to the next `\n` or the next `@<name>:` mention,
/// whichever comes first.
///
/// Grammar:
/// - The trigger char (`@` by default) announces the mention.
/// - The name is `[A-Za-z0-9_-]+` until a `:`.
/// - The prompt is everything after the `:` until the next newline
///   or the next mention start.
pub fn parse_mentions(text: &str, trigger: char) -> Vec<Mention> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = match text[i..].chars().next() {
            Some(c) => c,
            None => break,
        };
        if ch != trigger {
            i += ch.len_utf8();
            continue;
        }
        // Trigger found; try to read the name.
        let name_start = i + ch.len_utf8();
        let name_end = match text[name_start..].find(SEPARATOR_CHARS) {
            Some(off) => name_start + off,
            None => {
                // No colon follows the trigger; this is just a
                // literal `@` somewhere — skip.
                i += ch.len_utf8();
                continue;
            }
        };
        let agent = text[name_start..name_end].trim();
        if agent.is_empty()
            || !agent
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            i += ch.len_utf8();
            continue;
        }
        // Prompt starts after the colon.
        let prompt_start = name_end + 1;
        // Prompt ends at next `\n` or next `@`-trigger.
        let prompt_end = find_prompt_end(text, prompt_start, trigger);
        let prompt = text[prompt_start..prompt_end].trim().to_string();
        let end = prompt_end;
        out.push(Mention {
            agent: agent.to_string(),
            prompt,
            start: i,
            end,
        });
        i = end;
    }
    out
}

const SEPARATOR_CHARS: &str = ":";

fn find_prompt_end(text: &str, from: usize, trigger: char) -> usize {
    let bytes = text.as_bytes();
    let mut i = from;
    let mut prev_was_period = false;
    let mut next_was_space = false;
    while i < bytes.len() {
        let ch = match text[i..].chars().next() {
            Some(c) => c,
            None => break,
        };
        if ch == '\n' {
            return i;
        }
        if ch == trigger && i > from {
            return i;
        }
        // Sentence-end heuristic: `. ` followed by an uppercase
        // letter closes the prompt. This avoids the test case
        // `@x: do Y. Trailer.` swallowing the trailer.
        if prev_was_period && next_was_space && ch.is_ascii_uppercase() {
            return i.saturating_sub(1);
        }
        prev_was_period = ch == '.';
        next_was_space = ch == ' ';
        i += ch.len_utf8();
    }
    i
}

/// Remove the byte ranges of `mentions` from `text` and return
/// the surviving remainder (with the runs joined by single spaces
/// when removing a mention would otherwise leave two consecutive
/// whitespace runs).
fn strip_mentions(text: &str, mentions: &[Mention]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for m in mentions {
        if m.start < cursor {
            continue;
        }
        out.push_str(&text[cursor..m.start]);
        // Collapse the whitespace immediately before the mention
        // into a single space so the remainder doesn't glue
        // words together. We don't use `split_whitespace` on the
        // whole output because that would also eat trailing
        // whitespace, which matters for the LLM-facing
        // rendering.
        let trimmed = out.trim_end();
        out.truncate(trimmed.len());
        out.push(' ');
        cursor = m.end;
    }
    out.push_str(&text[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_mention() {
        let mentions = parse_mentions(
            "Hello @researcher: please summarise RFC 1234.",
            DEFAULT_TRIGGER,
        );
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].agent, "researcher");
        assert_eq!(mentions[0].prompt, "please summarise RFC 1234.");
    }

    #[test]
    fn parse_multiple_mentions() {
        let text = "Step 1: @researcher: find RFC.\nThen @reviewer: check it.";
        let mentions = parse_mentions(text, DEFAULT_TRIGGER);
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0].agent, "researcher");
        assert_eq!(mentions[0].prompt, "find RFC.");
        assert_eq!(mentions[1].agent, "reviewer");
        assert_eq!(mentions[1].prompt, "check it.");
    }

    #[test]
    fn parse_no_mention() {
        let mentions =
            parse_mentions("no trigger here, just plain text", DEFAULT_TRIGGER);
        assert!(mentions.is_empty());
    }

    #[test]
    fn parse_mention_with_email_like_text_kept_intact() {
        // The router only fires when the trigger is followed by a
        // valid name + colon. `user@example.com` has no colon,
        // so it stays as text.
        let mentions = parse_mentions(
            "Email user@example.com for context.",
            DEFAULT_TRIGGER,
        );
        assert!(mentions.is_empty());
    }

    #[test]
    fn leader_router_resolves_known_peers() {
        let router =
            LeaderRouter::new(["researcher", "reviewer"].map(String::from));
        let decision = router.route("Routing: @researcher: investigate X.");
        assert!(decision.has_mentions());
        let mentions = decision.mentions();
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].agent, "researcher");
    }

    #[test]
    fn leader_router_drops_unknown_peers() {
        let router = LeaderRouter::new(["researcher"].map(String::from));
        let decision = router.route("@unknown: do something.");
        // Unknown peer → no resolved mentions → pass-through.
        assert!(matches!(decision, RoutingDecision::PassThrough));
    }

    #[test]
    fn leader_router_mixed_resolved_and_unknown() {
        let router = LeaderRouter::new(["researcher"].map(String::from));
        let decision = router.route(
            "@researcher: find X. @stranger: do Y. @researcher: verify.",
        );
        let mentions = decision.mentions();
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0].agent, "researcher");
        assert_eq!(mentions[1].agent, "researcher");
    }

    #[test]
    fn pass_through_router_is_no_op() {
        let router = PassThroughRouter;
        let decision = router.route("@anyone: do anything.");
        assert!(matches!(decision, RoutingDecision::PassThrough));
    }

    #[test]
    fn remainder_drops_mention_runs() {
        // Single-line text where the prompt terminates at the
        // end-of-input (no following newline). The mention end
        // therefore equals the text length, so the trailer
        // gets eaten — and that's the documented behaviour
        // when the leader's prompt runs to end-of-input.
        let router = LeaderRouter::new(["researcher"].map(String::from));
        let text = "Header. @researcher: do X.";
        let decision = router.route(text);
        match decision {
            RoutingDecision::Route { remainder, .. } => {
                assert!(remainder.contains("Header."));
                assert!(!remainder.contains("do X."));
            }
            RoutingDecision::PassThrough => panic!("expected route"),
        }
    }
    #[test]
    fn custom_trigger_works() {
        let router =
            LeaderRouter::new(["x"].map(String::from)).with_trigger('#');
        let decision = router.route("Body #x: hello");
        assert!(decision.has_mentions());
    }

    #[test]
    fn empty_peers_never_routes() {
        let router: LeaderRouter = LeaderRouter::new(Vec::<String>::new());
        let decision = router.route("@researcher: do X.");
        assert!(matches!(decision, RoutingDecision::PassThrough));
    }

    #[test]
    fn mentions_convert_to_task_specs() {
        use crate::task::TaskSpec;
        let router =
            LeaderRouter::new(["researcher", "reviewer"].map(String::from));
        let decision =
            router.route("@researcher: find the RFC.\n@reviewer: check it.");
        let specs: Vec<TaskSpec> = mentions_to_task_specs(&decision);
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].agent, "researcher");
        assert_eq!(specs[0].prompt, "find the RFC.");
        assert_eq!(specs[1].agent, "reviewer");
        assert_eq!(specs[1].prompt, "check it.");
    }

    #[test]
    fn pass_through_converts_to_empty_specs() {
        let decision = RoutingDecision::PassThrough;
        assert!(mentions_to_task_specs(&decision).is_empty());
    }

    #[test]
    fn mention_clone_mode_defaults_to_false() {
        // The router is constructed via `LeaderRouter::new`; the
        // `mention_clone_mode` flag must default to `false` so
        // existing callers keep their `task`-tool dispatch.
        let router = LeaderRouter::new(["researcher"].map(String::from));
        assert!(!router.mention_clone_mode());
    }

    #[test]
    fn mention_clone_mode_setter_toggles() {
        let router = LeaderRouter::new(["researcher"].map(String::from))
            .with_mention_clone_mode(true);
        assert!(router.mention_clone_mode());
        // Toggling back to false round-trips.
        let router = router.with_mention_clone_mode(false);
        assert!(!router.mention_clone_mode());
    }
}
