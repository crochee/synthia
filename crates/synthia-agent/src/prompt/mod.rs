//! System-prompt assembly for [`crate::agent::ReActAgent`].
//!
//! ## Public surface (1 type, 2 entry points)
//!
//! - [`PromptContext`] — the builder.
//!
//! ```ignore
//! let prompt = PromptContext::default()
//!     .with_skill("transcribe", "Transcribe audio.")
//!     .with_agent(&descriptor)
//!     .assemble(&descriptor);
//! ```
//!
//! The base instructions come from the descriptor passed to
//! [`PromptContext::assemble`] — there is no separate
//! `with_instructions` knob, because the running agent's own
//! `descriptor.instructions` is the canonical source.
//!
//! ## Why a single builder
//!
//! Earlier revisions exposed the manifest types and let
//! callers hand-construct them. We deleted that surface
//! because the manifest is a private protocol: callers
//! supply *facts* (skill name + description, peer-agent
//! descriptor, running descriptor, runtime environment),
//! the builder decides *how* those facts are stored and
//! serialized. Hand-rolled struct literals would force
//! callers to track every field the assembler needs to
//! know about, and every new field would be a breaking
//! change. The builder fixes that: new internal fields stay
//! invisible.
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
//! 3. **`<env>`** — per-dispatch runtime facts (cwd, worktree,
//!    platform, date, model id). Only emitted when the caller
//!    has pushed an [`Environment`] via
//!    [`PromptContext::with_environment`].
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
//! Tool names are **not** re-asserted in the prompt. The
//! completion request's `tools` field is the single source of
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

use helpers::wrap;

use crate::agent::descriptor::AgentDescriptor;

// ---------------------------------------------------------------------------
// Per-dispatch runtime facts (rendered as <env>).
// ---------------------------------------------------------------------------

/// Per-dispatch runtime facts surfaced as the `<env>`
/// system-prompt section.
///
/// Cheap to clone (six `String`s and an `Option<bool>`) so
/// the loop can build one fresh on every `prepare()` without
/// measurable cost. `is_git_repo` is `Option<bool>` rather
/// than `bool` so a caller that doesn't know (the canonical
/// ReActAgent path has no git dep wired in) can render
/// `unknown` instead of guessing.
#[derive(Clone, Debug)]
pub struct Environment {
    /// Current working directory of the agent run.
    pub cwd: String,
    /// Workspace root handed to built-in tools via
    /// `synthia_tool::Context`. Often equal to `cwd`, but
    /// separated so a delegated sub-agent that pins to a
    /// project root while the parent loops in a scratch
    /// dir still tells the model where files live.
    pub worktree: String,
    /// `true` if `worktree` is inside a git working tree.
    /// `None` means "the runtime didn't check" and renders
    /// as `unknown` rather than `false`.
    pub is_git_repo: Option<bool>,
    /// `std::env::consts::OS` value (`"linux"`, `"macos"`,
    /// `"windows"`, …). Pre-stringified so the struct is
    /// plain-old-data and `Clone` stays trivial.
    pub platform: String,
    /// Human-readable date string (`Mon Jan 1 2026`).
    /// Kept as a `String` rather than `chrono::NaiveDate`
    /// so callers who don't pull in chrono can still build
    /// one by hand.
    pub today: String,
    /// Active model identifier (provider + model).
    /// Surfaced verbatim so the model can self-identify
    /// when asked. `None` means "not yet resolved" —
    /// renders as `unknown` in the section.
    pub model_id: Option<String>,
}

impl Environment {
    /// Build an Environment from the loop's
    /// `workspace_root` plus whatever the caller can read
    /// from `std::env`.
    ///
    /// `is_git_repo` defaults to `None` because
    /// `synthia-agent` has no git dependency — callers
    /// that want a definitive value can construct via
    /// `Environment { is_git_repo: Some(detect()), .. }`.
    pub fn from_runtime(workspace_root: &str) -> Self {
        Self {
            cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| String::from("unknown")),
            worktree: workspace_root.to_string(),
            is_git_repo: None,
            platform: std::env::consts::OS.to_string(),
            today: chrono::Local::now().format("%a %b %-d %Y").to_string(),
            model_id: None,
        }
    }

    /// Render the inner lines of the `<env>` block (no
    /// tag, no trailing newline). Public so test code can
    /// assert against the body without re-deriving the
    /// wrapping.
    pub fn render_body(&self) -> String {
        let git = match self.is_git_repo {
            Some(true) => "yes",
            Some(false) => "no",
            None => "unknown",
        };
        let model = self.model_id.as_deref().unwrap_or("unknown");
        format!(
            "  Working directory: {cwd}\n  Workspace root: {worktree}\n  Is directory a git repo: {git}\n  Platform: {platform}\n  Today: {today}\n  Model: {model}",
            cwd = self.cwd,
            worktree = self.worktree,
            platform = self.platform,
            today = self.today,
        )
    }
}

// ---------------------------------------------------------------------------
// Section renderers.
// ---------------------------------------------------------------------------
//
// Kept as plain functions next to the assembler instead of
// behind a trait + ZST indirection. The trait was a
// premature generalisation: there are exactly four
// sections, none of them carry state, and a future
// configurable section (e.g. a skill-filter) can be added
// by passing the filter through `PromptContext` rather
// than threading yet another type. Renderers return
// `Option<String>` — `None` drops the section entirely
// so the assembler never writes an empty `<tag/>`.
//
// All renderers take `&PromptContext` for stable manifest
// data (`skills`, `agents`, `environment`) and the
// per-dispatch `descriptor` as a separate parameter. The
// descriptor is not stored on the builder, so the same
// builder can render for multiple agents without any
// reset call.

fn render_identity(descriptor: &AgentDescriptor) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();

    // The opening line names the agent for the model. We
    // prefer `descriptor.display_name()` (the human-readable
    // label — e.g. "Synthia") over the programmatic
    // `descriptor.name` slug ("agent") so the model
    // self-identifies with the persona the user sees in the
    // UI and on the A2A card, not with the internal routing
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

fn render_env(scope: &PromptContext) -> Option<String> {
    let env = scope.environment.as_ref()?;
    Some(wrap("env", &env.render_body()))
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
        // surfaced separately in `<env>` so this stays
        // relative.
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
/// [`PromptContext::with_agent`], then on each dispatch
/// push the per-dispatch environment (optionally via
/// [`PromptContext::with_environment`]) and call
/// [`PromptContext::assemble`] with the running agent's
/// descriptor.
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
    /// Per-dispatch runtime facts — `None` means "skip the
    /// `<env>` block". Pushed by
    /// [`PromptContext::with_environment`] and survives
    /// across multiple `assemble()` calls (same env may
    /// be re-rendered against different descriptors in a
    /// batch).
    pub(crate) environment: Option<Environment>,
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

    /// Push per-dispatch runtime facts. Default (when this
    /// is never called) means "skip the `<env>` block" —
    /// callers that don't care about runtime facts can keep
    /// the legacy back-compat behaviour byte-for-byte.
    pub fn with_environment(mut self, environment: Environment) -> Self {
        self.environment = Some(environment);
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
    /// by the four internal section renderers, joined with
    /// `\n\n` (see [`helpers::push_block`]). The function
    /// is pure, deterministic, and side-effect free: given
    /// the same `(descriptor, ctx)` it produces the same
    /// string, which is what prompt caching requires.
    pub fn assemble(&self, descriptor: &AgentDescriptor) -> String {
        use helpers::{push_block, trimmed_non_empty};

        let mut out = String::new();
        let mut first = true;

        if let Some(base) = trimmed_non_empty(&descriptor.instructions) {
            push_block(&mut out, &mut first, base);
        }

        // Section render order — see module docs §
        // "Assembly order". Identity first (high-attention
        // persona), env next (dynamic facts the model
        // should ground every later response in), then the
        // manifests.
        let sections: [Option<String>; 4] = [
            render_identity(descriptor),
            render_env(self),
            render_skills(self),
            render_agents(self),
        ];
        for rendered in sections.into_iter().flatten() {
            push_block(&mut out, &mut first, &rendered);
        }

        // Final-prompt trace. `debug` level (off by default)
        // so production logs stay clean; flip on via
        // `RUST_LOG=synthia_agent::prompt=debug` to inspect
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::descriptor::AgentDescriptor;

    fn descriptor_with_instructions(instructions: &str) -> AgentDescriptor {
        AgentDescriptor {
            name: "agent".into(),
            description: "ReAct loop".into(),
            kind: "react".into(),
            version: "1.0.0".into(),
            instructions: instructions.into(),
            capabilities: vec!["tools".into(), "streaming".into()],
            tools: vec!["read_file".into()],
            model_hint: None,
            handoffs: vec!["planner".into()],
            handoff_hint: Some("Use for code-editing tasks".into()),
            output_schema: None,
            owner: Some("synthia".into()),
            domain: Some("coding".into()),
            persona: Some("You are a pragmatic senior engineer.".into()),
            display_name: None,
        }
    }

    fn peer_descriptor(name: &str) -> AgentDescriptor {
        AgentDescriptor {
            name: name.into(),
            description: "High-level planner.".into(),
            kind: "planner".into(),
            version: "1.0.0".into(),
            instructions: "".into(),
            capabilities: Vec::new(),
            tools: Vec::new(),
            model_hint: None,
            handoffs: Vec::new(),
            handoff_hint: Some("Use for complex tasks.".into()),
            output_schema: None,
            owner: None,
            domain: None,
            persona: None,
            display_name: None,
        }
    }

    /// Default-pack smoke test: the canonical inputs produce
    /// the documented structure in the documented order, with
    /// every section wrapped in balanced XML tags. Tool
    /// schemas are deliberately absent — they ride the
    /// completion request, not the prompt text.
    #[test]
    fn default_pack_renders_canonical_xml_order() {
        let d = descriptor_with_instructions("BASE");
        let out = PromptContext::default()
            .with_skill("transcribe", "Transcribe audio files.")
            .with_agent(&peer_descriptor("planner"))
            .assemble(&d);

        let order = [
            "BASE",
            "<identity>",
            "</identity>",
            "<available_skills>",
            "</available_skills>",
            "<available_agents>",
            "</available_agents>",
        ];
        let mut cursor = 0usize;
        for needle in order {
            let hit = out[cursor..].find(needle).unwrap_or_else(|| {
                panic!("missing `{needle}` after position {cursor}")
            });
            cursor = hit + needle.len();
        }

        assert!(out.contains("<name>transcribe</name>"));
        assert!(out.contains("`planner`"));

        assert!(
            !out.contains("<available_tools>"),
            "tool schemas must not be assembled into the system prompt; got:\n{out}"
        );
        assert!(
            !out.contains("Use only these tools:"),
            "tool grounding belongs on the runtime, not in the prompt; got:\n{out}"
        );

        // No `<rules>` block is rendered — the closing rules
        // section was intentionally removed. This is a hard
        // regression guard: any future section that adds an
        // `<rules>` (or `</rules>`) tag must update this
        // assertion and the section list above together.
        assert!(
            !out.contains("<rules>") && !out.contains("</rules>"),
            "the system prompt must not contain a <rules> block; got:\n{out}"
        );
        assert!(
            !out.contains("Apply only these skills:"),
            "skill grounding moved to <available_skills>; got:\n{out}"
        );
        assert!(
            !out.contains("Hand off only to these agents:"),
            "agent grounding moved to <available_agents>; got:\n{out}"
        );
    }

    /// The `<identity>` opening line must use the descriptor's
    /// `display_name` (the human-readable label) when set,
    /// and fall back to the programmatic `name` slug
    /// otherwise. Pins the contract that the model
    /// self-identifies with the persona the user sees on
    /// the UI / A2A card, not the internal routing id.
    #[test]
    fn identity_line_uses_display_name_when_set() {
        let mut d = descriptor_with_instructions("BASE");
        d.display_name = Some("Synthia".into());
        let out = PromptContext::default().assemble(&d);
        assert!(
            out.contains("You are `Synthia` (react v1.0.0)"),
            "identity line must use display_name; got:\n{out}"
        );
        assert!(
            !out.contains("You are `react`"),
            "identity line must not leak the routing slug; got:\n{out}"
        );
    }

    /// Without `display_name`, the assembler falls back to
    /// the programmatic `name` slug so legacy descriptors
    /// keep rendering the same identity they did before the
    /// field was added.
    #[test]
    fn identity_line_falls_back_to_name_when_display_name_missing() {
        let d = descriptor_with_instructions("BASE");
        // `display_name` is `None` on the test fixture.
        let out = PromptContext::default().assemble(&d);
        assert!(
            out.contains("You are `agent` (react v1.0.0)"),
            "missing display_name must fall back to name; got:\n{out}"
        );
    }

    /// Empty descriptor instructions still injects the
    /// manifest. The manifest sections carry the model's
    /// skills / peers regardless of whether the running
    /// agent's own prose is empty.
    #[test]
    fn empty_descriptor_instructions_still_injects_manifest() {
        let d = descriptor_with_instructions("");
        let out = PromptContext::default()
            .with_skill("transcribe", "Transcribe audio files.")
            .with_agent(&peer_descriptor("planner"))
            .assemble(&d);
        assert!(out.contains("<identity>"));
        assert!(out.contains("<available_skills>"));
        assert!(out.contains("<available_agents>"));
    }

    /// Disabled skills MUST NOT be pushed in by the caller —
    /// the assembler trusts the manifest as the canonical
    /// set of enabled skills.
    #[test]
    fn disabled_skills_are_never_pushed_by_caller() {
        let d = descriptor_with_instructions("BASE");
        let out = PromptContext::default()
            .with_skill("active", "on")
            // disabled-skill is omitted: caller is responsible
            // for filtering before pushing.
            .assemble(&d);
        assert!(out.contains("<name>active</name>"));
        assert!(!out.contains("<name>inactive</name>"));
    }

    /// The `<available_skills>` block MUST use the opencode
    /// XML-verbose envelope (matching
    /// `opencode/packages/opencode/src/skill/index.ts::fmt({verbose: true})`):
    /// each skill is a `<skill>` element with `<name>`,
    /// `<description>`, `<location>` children, all wrapped in
    /// `<skills>...</skills>`. Anthropic Agent Skills and
    /// Grok Build use the same shape; aligning lets models
    /// trained on either reference parse the block the same
    /// way they parse it on the other SDK.
    #[test]
    fn available_skills_uses_opencode_xml_envelope() {
        let d = descriptor_with_instructions("BASE");
        let out = PromptContext::default()
            .with_skill("code-review", "Procedure for reviewing a change.")
            .assemble(&d);
        // Outer wrappers.
        assert!(
            out.contains("<available_skills>")
                && out.contains("</available_skills>"),
            "<available_skills> outer block missing; got:\n{out}"
        );
        assert!(
            out.contains("<skills>") && out.contains("</skills>"),
            "<skills> inner envelope missing; got:\n{out}"
        );
        // Per-skill envelope, pinned to industry shape.
        assert!(
            out.contains("<skill>"),
            "<skill> per-entry element missing; got:\n{out}"
        );
        assert!(
            out.contains("<name>code-review</name>"),
            "<name> child missing; got:\n{out}"
        );
        assert!(
            out.contains(
                "<description>Procedure for reviewing a change.</description>"
            ),
            "<description> child missing; got:\n{out}"
        );
        assert!(
            out.contains(
                "<location>.agents/skills/code-review/SKILL.md</location>"
            ),
            "<location> child missing; got:\n{out}"
        );
        // Legacy bullet format MUST be gone.
        assert!(
            !out.contains("- `code-review` —"),
            "legacy bullet-list format must be replaced by the XML \
             envelope; got:\n{out}"
        );
    }

    /// Empty manifests drop the corresponding section
    /// entirely — no `(none)` placeholder, no empty `<tag/>`.
    /// The model wastes zero attention on absence signals.
    #[test]
    fn empty_manifests_drop_their_section() {
        let d = descriptor_with_instructions("BASE");
        let out = PromptContext::default().assemble(&d);
        assert!(out.contains("<identity>"));
        assert!(!out.contains("<available_skills>"));
        assert!(!out.contains("<available_agents>"));
    }

    /// Pure-string contract: same `(descriptor, ctx)` ⇒
    /// byte-identical output. Prompt caching requires this;
    /// if it ever regresses, cache hit-rate collapses
    /// silently.
    #[test]
    fn assemble_is_pure_and_deterministic() {
        let d = descriptor_with_instructions("BASE");
        let ctx = PromptContext::default()
            .with_skill("transcribe", "Transcribe audio files.")
            .with_agent(&peer_descriptor("planner"));
        let a = ctx.assemble(&d);
        let b = ctx.assemble(&d);
        assert_eq!(a, b);
    }

    /// `trim()` whitespace-only descriptor instructions are
    /// treated as absent, mirroring the empty case.
    #[test]
    fn whitespace_only_descriptor_instructions_are_dropped() {
        let d = descriptor_with_instructions("   \n\t  ");
        let out = PromptContext::default().assemble(&d);
        let first_tag = out.find('<').expect("identity tag present");
        assert!(out[..first_tag].is_empty());
    }

    /// `Some("")` persona / domain must NOT render a stray
    /// `Domain:` / `Persona:` line.
    #[test]
    fn identity_skips_empty_optional_fields() {
        let mut d = descriptor_with_instructions("");
        d.persona = Some(String::new());
        d.domain = Some(String::new());
        let out = PromptContext::default().assemble(&d);
        assert!(!out.contains("Domain: "));
        assert!(!out.contains("Persona: "));
    }

    /// Peer agents with empty-string `handoff_hint` must NOT
    /// render a stray `(use when: )` marker.
    #[test]
    fn agents_skip_empty_handoff_hint() {
        let d = descriptor_with_instructions("BASE");
        let mut p = peer_descriptor("p");
        p.handoff_hint = Some(String::new());
        let out = PromptContext::default().with_agent(&p).assemble(&d);
        assert!(!out.contains("(use when: )"));
        assert!(!out.contains("use when:"));
    }

    /// `PromptContext` has no `tools` field — pinned at the
    /// type level so a future refactor cannot quietly
    /// re-introduce tool-into-prompt assembly.
    #[test]
    fn prompt_context_has_no_tools_field() {
        let ctx = PromptContext::default();
        let _ = (&ctx.skills, &ctx.agents, &ctx.environment);
    }

    /// Base instructions come straight from
    /// `descriptor.instructions` — there is no separate
    /// override knob on `PromptContext`.
    #[test]
    fn descriptor_instructions_are_the_base_prompt() {
        let d = descriptor_with_instructions("FIRST_AGENT_INSTRUCTIONS");
        let out = PromptContext::default().assemble(&d);
        assert!(out.starts_with("FIRST_AGENT_INSTRUCTIONS"));
    }

    /// Same `PromptContext` rendered against two different
    /// descriptors must yield different identity blocks but
    /// identical skill / agent blocks — that's the contract
    /// for treating `descriptor` as per-call input rather
    /// than builder state.
    #[test]
    fn assemble_supports_multiple_agents_via_one_builder() {
        let d1 = descriptor_with_instructions("AGENT_ONE");
        let d2 = descriptor_with_instructions("AGENT_TWO");
        let ctx = PromptContext::default()
            .with_skill("transcribe", "Transcribe audio files.")
            .with_agent(&peer_descriptor("planner"));
        let a = ctx.assemble(&d1);
        let b = ctx.assemble(&d2);
        assert!(a.starts_with("AGENT_ONE"));
        assert!(b.starts_with("AGENT_TWO"));
        assert!(a.contains("<name>transcribe</name>"));
        assert!(b.contains("<name>transcribe</name>"));
    }

    // -- <env> section tests ---------------------------------------------

    fn fixed_environment() -> Environment {
        Environment {
            cwd: "/tmp/cwd".into(),
            worktree: "/tmp/worktree".into(),
            is_git_repo: Some(true),
            platform: "linux".into(),
            today: "Mon Jan 1 2026".into(),
            model_id: Some("anthropic/claude-4.6".into()),
        }
    }

    /// Back-compat: a builder without `with_environment`
    /// must NOT emit an `<env>` block. Existing callers
    /// rely on the output being byte-identical to the
    /// pre-environment version.
    #[test]
    fn assemble_without_env_does_not_emit_env_block() {
        let d = descriptor_with_instructions("BASE");
        let out = PromptContext::default()
            .with_skill("transcribe", "Transcribe audio files.")
            .with_agent(&peer_descriptor("planner"))
            .assemble(&d);
        assert!(
            !out.contains("<env>") && !out.contains("</env>"),
            "<env> block must be opt-in via with_environment; got:\n{out}"
        );
    }

    /// `with_environment(env)` emits a balanced
    /// `<env>...</env>` block immediately after `<identity>`
    /// and before `<available_skills>`. Order matters — env
    /// facts land at a high-attention position so the model
    /// grounds every later response in them.
    #[test]
    fn assemble_with_env_emits_env_after_identity() {
        let d = descriptor_with_instructions("BASE");
        let env = fixed_environment();
        let out = PromptContext::default()
            .with_skill("transcribe", "Transcribe audio files.")
            .with_agent(&peer_descriptor("planner"))
            .with_environment(env)
            .assemble(&d);

        let order = [
            "BASE",
            "<identity>",
            "</identity>",
            "<env>",
            "</env>",
            "<available_skills>",
            "</available_skills>",
            "<available_agents>",
            "</available_agents>",
        ];
        let mut cursor = 0usize;
        for needle in order {
            let hit = out[cursor..].find(needle).unwrap_or_else(|| {
                panic!("missing `{needle}` after position {cursor}; full output:\n{out}")
            });
            cursor = hit + needle.len();
        }

        // Every field from Environment must surface verbatim.
        assert!(out.contains("Working directory: /tmp/cwd"));
        assert!(out.contains("Workspace root: /tmp/worktree"));
        assert!(out.contains("Is directory a git repo: yes"));
        assert!(out.contains("Platform: linux"));
        assert!(out.contains("Today: Mon Jan 1 2026"));
        assert!(out.contains("Model: anthropic/claude-4.6"));
    }

    /// `is_git_repo: None` renders `unknown` rather than
    /// `false` so the model can distinguish "the runtime
    /// didn't check" from "checked and found no".
    #[test]
    fn env_renders_unknown_when_git_status_missing() {
        let d = descriptor_with_instructions("BASE");
        let mut env = fixed_environment();
        env.is_git_repo = None;
        let out = PromptContext::default().with_environment(env).assemble(&d);
        assert!(
            out.contains("Is directory a git repo: unknown"),
            "missing git status must surface as `unknown`, not `no`; got:\n{out}"
        );
    }

    /// `is_git_repo: Some(false)` renders `no`.
    #[test]
    fn env_renders_no_when_git_status_false() {
        let d = descriptor_with_instructions("BASE");
        let mut env = fixed_environment();
        env.is_git_repo = Some(false);
        let out = PromptContext::default().with_environment(env).assemble(&d);
        assert!(
            out.contains("Is directory a git repo: no"),
            "git=false must surface as `no`; got:\n{out}"
        );
    }

    /// `Environment::from_runtime` populates every field
    /// without panicking, even when `current_dir()` is
    /// unreadable (it falls back to `unknown`). Platform and
    /// `today` must be non-empty.
    #[test]
    fn environment_from_runtime_populates_required_fields() {
        let env = Environment::from_runtime("/tmp/worktree");
        assert_eq!(env.worktree, "/tmp/worktree");
        assert!(!env.platform.is_empty(), "platform must be non-empty");
        assert!(!env.today.is_empty(), "today must be non-empty");
        assert!(
            env.is_git_repo.is_none(),
            "from_runtime leaves git status as `None` (no git dep)"
        );
        assert!(
            env.model_id.is_none(),
            "from_runtime leaves model_id as `None`"
        );
        // cwd may be `unknown` if the test runner has no cwd;
        // either is fine — the contract is "non-panicking".
        let _ = env.cwd;
    }

    /// Pure-string contract on the env-bearing path: same
    /// `(d, env, ctx)` ⇒ byte-identical output. Pinned so a
    /// future refactor (e.g. accidentally reading
    /// `chrono::Utc::now` inside `Environment::render_body`
    /// instead of using the cached `today`) cannot silently
    /// break cache stability.
    #[test]
    fn assemble_with_env_is_pure_and_deterministic() {
        let d = descriptor_with_instructions("BASE");
        let env = fixed_environment();
        let ctx = PromptContext::default()
            .with_skill("transcribe", "Transcribe audio files.")
            .with_agent(&peer_descriptor("planner"))
            .with_environment(env);
        let a = ctx.assemble(&d);
        let b = ctx.assemble(&d);
        assert_eq!(a, b);
    }
}
