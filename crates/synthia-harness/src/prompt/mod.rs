//! System-prompt assembly for [`crate::agent::ReActAgent`].
//!
//! ## Public surface (2 types, 2 entry points)
//!
//! - [`PromptContext`] — the system-prompt builder.
//!
//! ```ignore
//! let prompt = PromptContext::default()
//!     .with_skill("transcribe", "Transcribe audio.")
//!     .with_agent(&descriptor)
//!     .assemble(&descriptor);
//! ```
//!
//! - [`runtime_context::RuntimeContext`] — the
//!   cache-stable snapshot rendered as a user-role message
//!   at every iteration. Volatile facts (cwd, today, model
//!   id) live here, NOT in the system prompt, so the
//!   provider's prompt cache stays hot across a session.
//!
//! The base instructions come from the descriptor passed to
//! [`PromptContext::assemble`] — there is no separate
//! `with_instructions` knob, because the running agent's own
//! `descriptor.instructions` is the canonical source.
//!
//! ## Why a single builder
//!
//! callers hand-construct them. We deleted that surface
//! because the manifest is a private protocol: callers
//! supply *facts* (skill name + description, peer-agent
//! descriptor), the builder decides *how* those facts are
//! stored and serialized. Hand-rolled struct literals
//! would force callers to track every field the assembler
//! needs to know about, and every new field would be a
//! breaking change. The builder fixes that: new internal
//! fields stay invisible.
//!
//! Volatile runtime facts (cwd, today, model id) live on
//! a separate seam — [`runtime_context::RuntimeContext`] —
//! so the system prompt stays byte-stable across a session
//! and the provider's prompt cache stays hot.

//!
//! ## Why a dedicated module?
//!
//! The ReAct loop's first message tells the LLM (1) its persona,
//! (2) which skills it can apply, and (3) which peer agents it
//! can hand off to. Tool
//! schemas are **not** part of the system prompt — they are
//! delivered on the provider-native `tools` channel of the
//! completion request, which every modern API supports
//! (Anthropic `tools`, OpenAI `tools`, Google `tools`). The
//! model reads the schema where the API hands it the schema.
//!
//! Industry reference designs (Anthropic Agent SDK, OpenAI
//! Agents SDK / Swarm, Google ADK) all converge on the same
//! shape: a deterministic, XML-delimited assembly whose section
//! order puts persona + capabilities at the high-attention
//! edges (see "Lost in the Middle", Liu et al. 2024).
//!
//! ## Why XML tags?
//!
//! Anthropic explicitly recommends `<role>` / `<skills>` style
//! tags. Claude treats them as structural
//! anchors rather than prose — the model separates "instruction
//! context" from "manifest data" by tag, which produces
//! measurably more reliable output. The same vocabulary works
//! across providers: GPT and Gemini both handle XML delimiters
//! well, and OpenAI's guide flags Markdown headings as weaker
//! than XML for long prompts.
//!
//! ## Assembly order
//!
//! 1. **Base instructions** — `descriptor.instructions`
//!    verbatim. The descriptor is the canonical source of the
//!    running agent's persona / tone / project-specific
//!    guidance; there is no separate `with_instructions` knob
//!    because the agent's own descriptor already carries
//!    that payload.
//! 2. **`<identity>`** — descriptor metadata (name, kind,
//!    persona, domain, capabilities). Always emitted when
//!    non-empty so the model always knows who it is.
//! 3. **`<structured_output>`** — the mandate to finish by
//!    calling the `structured_output` tool, emitted only when
//!    `descriptor.output_schema` is set. It sits with the
//!    identity block rather than with the manifests because it
//!    is an *instruction*, not a catalogue: it pairs with the
//!    persona half of the prompt (both say what this run must
//!    do) and precedes the skills / agent menus (which say what
//!    is available). The schema text itself is **not** inlined;
//!    it is the tool's `parameters()`, delivered on the native
//!    `tools` channel like every other tool.
//! 4. **`<available_skills>`** — every enabled skill's name +
//!    description + on-disk location, wrapped in an XML
//!    `<skills>` envelope (opencode / Anthropic Agent Skills
//!    verbose convention). Skills are agent-runtime concepts
//!    the model must read about in prose to apply them, so
//!    unlike tools they stay in the prompt text.
//! 5. **`<available_agents>`** — every other registered
//!    agent's name + description + handoff hint. Routes the
//!    model to the right peer for delegation.
//!
//! Tool names are **not** re-asserted in the prompt.
//! The completion request's `tools` field is the single source of
//! truth for what the model can call, and the runtime validates
//! every emitted tool name against the registry before
//! dispatching — duplicating the names in prose would invite
//! drift without adding any safety the runtime layer doesn't
//! already provide.
//!
//! Empty sections are dropped (no `(none)` placeholder). That
//! saves tokens and, more importantly, prevents the model from
//! anchoring on the absence of a manifest as a signal.
//!
//! Sections are joined with `\n\n`. Stable content — descriptor
//! and manifests — is delimited by XML tags so it can be cached
//! by providers that support prompt caching (Anthropic, OpenAI
//! automatic caching).

mod helpers;
pub mod runtime_context;

use helpers::wrap;
pub use runtime_context::RuntimeContext;

use crate::agent::AgentDescriptor;

// ---------------------------------------------------------------------------
// Per-dispatch runtime facts (rendered as a user-role snapshot
// by the loop, NOT in the system prompt — see
// `prompt::runtime_context::RuntimeContext`).
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Section renderers.
// ---------------------------------------------------------------------------
// Kept as plain functions next to the assembler instead of
// behind a trait + ZST indirection. The trait was a
// premature generalisation: there are exactly three
// sections, none of them carry state, and a future
// configurable section (e.g. a skill-filter) can be added
// by passing the filter through `PromptContext` rather
// than threading yet another type. Renderers return
// `Option<String>` — `None` drops the section entirely
// so the assembler never writes an empty `<tag/>`.
//
// All renderers take `&PromptContext` for stable manifest
// data (`skills`, `agents`) and the per-dispatch
// `descriptor` as a separate parameter. The descriptor is
// not stored on the builder, so the same builder can render
// for multiple agents without any reset call.

fn render_identity(descriptor: &AgentDescriptor) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();

    // The opening line names the agent for the model. We
    // prefer `descriptor.display_name()` (the human-readable
    // label — e.g. "Synthia") over the programmatic
    // `descriptor.name` slug ("agent") so the model
    // self-identifies with the persona the user sees in the
    // UI card, not with the internal routing
    // id. The fallback inside `display_name()` guarantees
    // legacy descriptors (no `display_name` set) still
    // render the same identity they did before the field
    // was added.
    lines.push(format!(
        "You are `{}` ({} v{}).",
        descriptor.display_name(),
        descriptor.kind,
        descriptor.version
    ));
    if let Some(persona) =
        descriptor.persona.as_deref().filter(|s| !s.is_empty())
    {
        lines.push(format!("Persona: {persona}"));
    }
    if let Some(domain) = descriptor.domain.as_deref().filter(|s| !s.is_empty())
    {
        lines.push(format!("Domain: {domain}"));
    }
    if !descriptor.capabilities.is_empty() {
        lines.push(format!(
            "Capabilities: {}",
            descriptor.capabilities.join(", ")
        ));
    }

    // If the descriptor is genuinely bare (no name, no
    // persona, no capabilities, etc.) emit nothing — the
    // upstream `<identity>` tag would carry an empty body
    // and waste tokens.
    if lines.is_empty() {
        return None;
    }

    Some(wrap("identity", &lines.join("\n")))
}

/// Tell the model that this run must finish by calling
/// `structured_output`, when the descriptor declares a schema.
///
/// The schema itself never enters the prompt: `descriptor.output_schema`
/// is the tool's `parameters()`, so the provider already delivers it on
/// the native `tools` channel, exactly as the module docs describe for
/// every other tool. What the tool channel cannot say is that this one is
/// *mandatory* — a tool list is a menu, not an instruction, and a model
/// handed a schema-bearing tool is just as free to answer in prose. So
/// the prompt carries the one fact the wire cannot: that prose is not an
/// accepted ending here.
///
/// This is what makes the finalize-time check meaningful. Without it the
/// warning would fire on nearly every schema-declaring run — the model
/// was never asked to submit — which is noise rather than validation.
fn render_structured_output(descriptor: &AgentDescriptor) -> Option<String> {
    let schema = descriptor.output_schema.as_deref()?;
    if schema.trim().is_empty() {
        return None;
    }
    Some(wrap(
        "structured_output",
        "This run must end by calling the `structured_output` tool with \
         every field filled exactly as its schema requires. Do not answer \
         in prose instead: the caller reads the tool's validated payload, \
         and a prose answer leaves it with no result. If validation fails, \
         correct the listed fields and call the tool again.",
    ))
}

fn render_skills(scope: &PromptContext) -> Option<String> {
    if scope.skills.is_empty() {
        return None;
    }
    // Aligned with the opencode `<available_skills>` verbose
    // format (`opencode/packages/opencode/src/skill/index.ts::
    // fmt({ verbose: true })`). Each skill is a `<skill>`
    // element with `<name>` / `<description>` / `<location>`
    // children. Anthropic / Grok Build use a similar envelope;
    // matching it lets the model parse the block the same way
    // it parses the same shape on every other agent SDK.
    let mut lines: Vec<String> = Vec::with_capacity(scope.skills.len() * 4 + 2);
    lines.push(
        "Load a skill with the `skill` tool when the task at \
         hand matches its description."
            .into(),
    );
    lines.push("<skills>".into());
    for (name, description) in &scope.skills {
        let desc = if description.is_empty() {
            "(no description)"
        } else {
            description.as_str()
        };
        // `location` is the canonical relative path the
        // model uses to reason about where on disk the
        // skill lives. The agent's `workspace_root` is
        // surfaced in the runtime-context snapshot, so
        // this stays relative.
        lines.push("  <skill>".into());
        lines.push(format!("    <name>{name}</name>"));
        lines.push(format!("    <description>{desc}</description>"));
        lines.push(format!(
            "    <location>.agents/skills/{name}/SKILL.md</location>"
        ));
        lines.push("  </skill>".into());
    }
    lines.push("</skills>".into());
    Some(wrap("available_skills", &lines.join("\n")))
}

fn render_agents(scope: &PromptContext) -> Option<String> {
    if scope.agents.is_empty() {
        return None;
    }
    let mut lines: Vec<String> = Vec::with_capacity(scope.agents.len() + 1);
    lines.push(
        "Hand off to one of these agents when their description fits:".into(),
    );
    for a in &scope.agents {
        let desc = if a.description.is_empty() {
            "(no description)"
        } else {
            a.description.as_str()
        };
        let hint = a
            .handoff_hint
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| format!(" (use when: {s})"))
            .unwrap_or_default();
        lines.push(format!("- `{name}` — {desc}{hint}", name = a.name));
    }
    Some(wrap("available_agents", &lines.join("\n")))
}

// ---------------------------------------------------------------------------
// Public builder.
// ---------------------------------------------------------------------------

/// Builder for the system prompt manifest, plus the only
/// place in the crate where the assembler can be invoked.
///
/// Construct one with `PromptContext::default()`, feed
/// stable facts via [`PromptContext::with_skill`] and
/// [`PromptContext::with_agent`], then call
/// [`PromptContext::assemble`] with the running agent's
/// descriptor. The system prompt is byte-stable across
/// dispatches — only the manifest sections render; per-dispatch
/// volatile facts (cwd, today, model id) live in the runtime
/// context snapshot seam and never appear in this output.
///
/// Stable fields (`skills`, `agents`) live in the builder
/// across dispatches — they're cheap to clone and re-used
/// every call. The descriptor is **not** stored on the
/// builder; it is passed straight to
/// [`PromptContext::assemble`] each call. That keeps the
/// same builder reusable across agents in one process
/// without any per-agent reset, and keeps the builder's
/// lifetime detached from the caller's descriptor
/// reference.
///
/// `skills` is a tuple `(name, description)` — disabled
/// skills are not pushed in by the caller, so the manifest
/// contains only skills the model may apply.
///
/// `agents` holds full [`AgentDescriptor`]s; the assembler
/// extracts `name`, `description`, and `handoff_hint` and
/// ignores the rest.
///
/// **No tools field.** Tool schemas are not part of the system
/// prompt — they travel on the completion request's `tools`
/// field, which is the API-native channel for tool
/// declarations. See module docs § "Why a dedicated module?".
#[derive(Clone, Debug, Default)]
pub struct PromptContext {
    pub(crate) skills: Vec<(String, String)>,
    pub(crate) agents: Vec<AgentDescriptor>,
}

impl PromptContext {
    /// Append one skill. Disabled skills are not pushed in by
    /// the caller, so a `with_skill` call means "the model
    /// may apply this skill".
    pub fn with_skill(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        self.skills.push((name.into(), description.into()));
        self
    }

    /// Append one peer agent. The assembler extracts
    /// `name`, `description`, and `handoff_hint` from the
    /// descriptor; everything else is ignored.
    pub fn with_agent(mut self, descriptor: &AgentDescriptor) -> Self {
        self.agents.push(descriptor.clone());
        self
    }

    /// Render the final system prompt.
    ///
    /// `descriptor` is the canonical source of both the
    /// running agent's identity (`<identity>` block) and
    /// its base instructions (the leading prose block).
    /// It is passed per call rather than stored on the
    /// builder so the same `PromptContext` can render for
    /// any agent without a `set_descriptor` reset.
    ///
    /// The output is a single string assembled from the
    /// base prompt and the XML-delimited sections returned
    /// by the four internal section renderers (identity,
    /// structured-output mandate, skills, agents — see module
    /// docs § "Assembly order"), joined with `\n\n` (see
    /// `helpers::push_block`). The
    /// function is pure, deterministic, and side-effect
    /// free: given the same `(descriptor, ctx)` it
    /// produces the same string, which is what prompt
    /// caching requires.
    pub fn assemble(&self, descriptor: &AgentDescriptor) -> String {
        use helpers::{push_block, trimmed_non_empty};

        let mut out = String::new();
        let mut first = true;

        if let Some(base) = trimmed_non_empty(&descriptor.instructions) {
            push_block(&mut out, &mut first, base);
        }
        // Section render order — see module docs §
        // "Assembly order". Identity first (high-attention
        // persona), then the manifests. Per-dispatch
        // volatile facts (cwd, today, model id) live in
        // the runtime-context snapshot seam and never
        // appear in this output — keeping the system
        // prompt byte-stable across a session so the
        // provider's prompt cache hits every turn.
        let sections: [Option<String>; 4] = [
            render_identity(descriptor),
            render_structured_output(descriptor),
            render_skills(self),
            render_agents(self),
        ];
        for rendered in sections.into_iter().flatten() {
            push_block(&mut out, &mut first, &rendered);
        }

        // Final-prompt trace. `debug` level (off by default)
        // so production logs stay clean; flip on via
        // `RUST_LOG=synthia_harness::prompt=debug` to inspect
        // exactly what the model sees. The `agent` field is
        // the descriptor name — same field name used
        // elsewhere in `re_act.rs::prepare` so log search
        // filters carry across modules.
        tracing::debug!(
            agent = %descriptor.name,
            bytes = out.len(),
            "assembled system prompt:\n{}",
            out
        );

        out
    }
}
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
//
// The test suite is split into focused submodules under
// `tests/`. Each submodule covers one concern: the public
// assemble contract, the `<identity>` renderer, the
// `<available_skills>` / `<available_agents>` renderers, and
// the runtime-context snapshot seam. The shared test fixtures
// live in `tests::support` so the split files keep the same
// one-fixture-per-file shape.
#[cfg(test)]
mod tests;
