use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Frontmatter of a `SKILL.md` file.
///
/// Aligned with the Anthropic Agent Skills open standard
/// ([`agentskills.io`](https://agentskills.io/specification)):
/// `name` is required; `description` is recommended but
/// optional; everything else is open-ended metadata.
///
/// Industry reference points:
///
/// - **Anthropic Agent Skills**: `name` (required, ≤64 chars,
///   lowercase letters / digits / hyphens), `description`
///   (recommended, ≤1024 chars, must describe both *what* and
///   *when to use*).
/// - **OpenAI Codex CLI**: same frontmatter shape, `name` only
///   is required.
/// - **GitHub Copilot / OpenCode / Grok Build**: same
///   frontmatter shape; OpenCode uses the directory name when
///   the `name` field is absent.
///
/// The body of `SKILL.md` (everything after the `---` closer)
/// is loaded by [`crate::loader::SkillLoader::parse_body`] and
/// exposed to the runtime as a plain `String`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Skill identifier. Required. Matches the parent
    /// directory name (verified at load time by
    /// [`crate::loader::SkillLoader::parse_frontmatter`]).
    pub name: String,
    /// One-line description surfaced through the
    /// `<available_skills>` prompt block so the model knows
    /// when to invoke the skill. Optional — the model
    /// falls back to the first non-empty line of the body
    /// when this is absent, matching the Anthropic /
    /// OpenCode convention.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Extension bag: projects can add custom keys without
    /// changing this struct. Anthropic / OpenCode /
    /// Grok Build all reserve a `metadata:` map for
    /// vendor-specific fields (`allowed-tools`,
    /// `category`, etc.).
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SkillMetadata` MUST round-trip through
    /// YAML identity — the same fields read back
    /// as the same struct. This is the loading
    /// contract for `SKILL.md` frontmatter.
    #[test]
    fn round_trip_through_yaml_is_identity() {
        let original = SkillMetadata {
            name: "code-review".to_string(),
            description: Some("Review code".to_string()),
            metadata: HashMap::new(),
        };
        let yaml = serde_yaml::to_string(&original).expect("serialize");
        let parsed: SkillMetadata = serde_yaml::from_str(&yaml).expect("parse");
        assert_eq!(parsed.name, original.name);
        assert_eq!(parsed.description.as_deref(), Some("Review code"));
        assert!(parsed.metadata.is_empty());
    }

    /// `name` is a required field — missing it
    /// MUST yield a deserialize error, not an
    /// empty-string field. Pin the contract so a
    /// refactor that adds `#[serde(default)]` to
    /// `name` (which would silently accept
    /// nameless skills) breaks loudly.
    #[test]
    fn missing_name_field_yields_deserialize_error() {
        let yaml = "description: only description\n";
        let err = serde_yaml::from_str::<SkillMetadata>(yaml).unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("name")
                || err.to_string().to_lowercase().contains("missing"),
            "error must mention name or missing; got {err}"
        );
    }

    /// `description` is OPTIONAL — aligned with the
    /// Anthropic Agent Skills open standard, where only
    /// `name` is required. Missing `description` MUST
    /// deserialize into `None`, not panic or yield
    /// an empty-string.
    #[test]
    fn missing_description_field_yields_none() {
        let yaml = "name: x\n";
        let parsed: SkillMetadata = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(parsed.name, "x");
        assert!(
            parsed.description.is_none(),
            "missing description must deserialize as None; got {:?}",
            parsed.description
        );
    }

    /// `description: ""` (explicit empty) deserialises as
    /// `Some("")` — the YAML layer preserves the empty
    /// string. The loader (`SkillLoader::parse_frontmatter`)
    /// is what normalises blank strings to `None` so callers
    /// don't need to distinguish "missing" from "blank".
    /// Pin both: raw deserialisation preserves, the loader
    /// normalises.
    #[test]
    fn empty_description_is_preserved_through_yaml() {
        let yaml = "name: x\ndescription: \"\"\n";
        let parsed: SkillMetadata = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(
            parsed.description.as_deref(),
            Some(""),
            "serde_yaml preserves empty strings; normalisation \
             happens in SkillLoader, not at the type layer"
        );
    }

    /// `metadata` is `#[serde(default)]` — a
    /// frontmatter without `metadata:` MUST
    /// deserialize into an empty HashMap, not
    /// panic. This is the forward-compat
    /// contract that lets skills opt out of
    /// metadata cleanly.
    #[test]
    fn missing_metadata_defaults_to_empty_hashmap() {
        let yaml = "name: x\ndescription: y\n";
        let parsed: SkillMetadata = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(parsed.metadata.len(), 0);
    }

    /// `metadata` is also a valid round-trip
    /// for arbitrary JSON values — this is the
    /// extension point projects use to add
    /// custom fields without changing this
    /// struct.
    #[test]
    fn metadata_with_arbitrary_json_values_round_trips() {
        let yaml = r#"
name: x
description: y
metadata:
  category: tooling
  priority: 5
  enabled: true
  tags: [rust, ai]
"#;
        let parsed: SkillMetadata = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(
            parsed.metadata.get("category").unwrap(),
            &serde_json::json!("tooling")
        );
        assert_eq!(
            parsed.metadata.get("priority").unwrap(),
            &serde_json::json!(5)
        );
        assert_eq!(
            parsed.metadata.get("enabled").unwrap(),
            &serde_json::json!(true)
        );
        // YAML arrays become JSON arrays.
        assert_eq!(
            parsed.metadata.get("tags").unwrap(),
            &serde_json::json!(["rust", "ai"])
        );
    }

    /// JSON round-trip MUST also work (forward-
    /// compat with non-YAML frontmatter
    /// dialects, e.g. `.json` skill manifests).
    #[test]
    fn round_trip_through_json_is_identity() {
        let original = SkillMetadata {
            name: "x".to_string(),
            description: Some("y".to_string()),
            metadata: HashMap::new(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: SkillMetadata = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed.name, original.name);
        assert_eq!(parsed.description.as_deref(), Some("y"));
    }

    /// `description: None` serialises WITHOUT the
    /// `description` key (Anthropic convention —
    /// a missing key reads back as missing, not
    /// as `null`).
    #[test]
    fn missing_description_does_not_serialize_as_null() {
        let m = SkillMetadata {
            name: "x".to_string(),
            description: None,
            metadata: HashMap::new(),
        };
        let yaml = serde_yaml::to_string(&m).expect("serialize");
        assert!(
            !yaml.contains("description"),
            "missing description must not serialise; got:\n{yaml}"
        );
    }
}
