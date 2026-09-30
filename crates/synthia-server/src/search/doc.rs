//! [`SearchDoc`] — the server's one searchable record shape.
//!
//! Every static catalog domain (tool / mcp / skill / agent) projects
//! its rows into this single type, so the whole wrap lives in
//! `synthia-server` and no `synthia-*` library crate learns about
//! search. The two corpus domains (memory / session) do not use it:
//! they delegate to their own retrievers behind
//! [`ErasedEngine`](synthia::search::ErasedEngine) adapters.

use synthia::{
    core::registry::RegistryItem,
    search::Searchable,
    skill::Skill as SkillFile,
    tool::ToolDescriptor,
};

/// One indexable catalog row.
#[derive(Clone)]
pub(crate) struct SearchDoc {
    id: String,
    title: String,
    body: String,
    when_to_use: Vec<String>,
    tags: Vec<String>,
}

impl SearchDoc {
    /// A tool (or MCP tool) descriptor: the name is the identity,
    /// the description is the payload, the parameter names make
    /// "which tool takes a `path` argument" findable.
    pub(crate) fn from_tool_descriptor(d: &ToolDescriptor) -> Self {
        let params = d
            .parameters
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|m| m.keys().cloned().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        Self {
            id: d.name.clone(),
            title: first_line(&d.description),
            body: format!("{} {}", d.description, params),
            when_to_use: Vec::new(),
            tags: vec![format!("{:?}", d.category).to_lowercase()],
        }
    }

    /// A discovered `SKILL.md`: name + frontmatter description are
    /// the catalog surface; the body content is what makes a skill
    /// findable by *what it does*, not just by its title.
    pub(crate) fn from_skill(s: &SkillFile) -> Self {
        Self {
            id: format!("skill:{}", s.name),
            title: s.effective_description(),
            body: s.content.clone(),
            when_to_use: Vec::new(),
            tags: Vec::new(),
        }
    }

    /// A registered peer agent: identity, routing hint and persona
    /// are exactly what an orchestrator searches for when deciding
    /// who to hand a task to.
    pub(crate) fn from_agent_descriptor(
        d: &synthia::core::agent::AgentDescriptor,
    ) -> Self {
        let mut when_to_use = Vec::new();
        if let Some(hint) = d.handoff_hint.as_deref().filter(|s| !s.is_empty())
        {
            when_to_use.push(hint.to_string());
        }
        if let Some(persona) = d.persona.as_deref().filter(|s| !s.is_empty()) {
            when_to_use.push(persona.to_string());
        }
        Self {
            id: format!("agent:{}", d.name),
            title: first_line(&d.description),
            body: format!(
                "{} {} {}",
                d.description,
                d.capabilities.join(" "),
                d.tools.join(" ")
            ),
            when_to_use,
            tags: vec![d.kind.clone()],
        }
    }
}

fn first_line(s: &str) -> String {
    let line = s.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    line.chars().take(120).collect()
}

impl RegistryItem for SearchDoc {
    fn name(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.title
    }
}

impl Searchable for SearchDoc {
    fn indexed_fields(&self) -> Vec<(String, f32)> {
        let mut fields =
            vec![(self.title.clone(), 2.5), (self.body.clone(), 1.0)];
        if !self.when_to_use.is_empty() {
            fields.push((self.when_to_use.join(" "), 1.5));
        }
        if !self.tags.is_empty() {
            fields.push((self.tags.join(" "), 1.0));
        }
        fields
    }

    fn when_to_use(&self) -> &[String] {
        &self.when_to_use
    }

    fn tags(&self) -> &[String] {
        &self.tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_descriptor(name: &str, description: &str) -> ToolDescriptor {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": { "path": {}, "contents": {} }
            },
            "category": "utility",
            "is_hidden": false,
            "exposure": "direct"
        }))
        .expect("valid ToolDescriptor")
    }

    #[test]
    fn tool_doc_indexes_name_description_and_params() {
        let doc = SearchDoc::from_tool_descriptor(&tool_descriptor(
            "write",
            "Write a file\nunder the workspace root",
        ));
        assert_eq!(doc.name(), "write");
        assert_eq!(doc.description(), "Write a file");
        let fields = doc.indexed_fields();
        assert!(fields.contains(&("Write a file".to_string(), 2.5)));
        // Body carries the full description + the parameter names,
        // so "which tool takes `contents`" is answerable.
        assert!(fields.iter().any(|(text, _)| text.contains("contents")));
    }

    #[test]
    fn agent_doc_carries_handoff_hint_as_when_to_use() {
        let d: synthia::core::agent::AgentDescriptor =
            serde_json::from_value(serde_json::json!({
                "name": "reviewer",
                "description": "Strict code reviewer",
                "kind": "specialist",
                "version": "1",
                "handoff_hint": "use when: a diff needs review"
            }))
            .expect("valid AgentDescriptor");
        let doc = SearchDoc::from_agent_descriptor(&d);
        assert_eq!(
            doc.when_to_use(),
            &["use when: a diff needs review".to_string()]
        );
        assert_eq!(doc.name(), "agent:reviewer");
        assert_eq!(doc.description(), "Strict code reviewer");
    }
}
