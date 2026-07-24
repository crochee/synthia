//! Filesystem-driven agent definition loading.
//!
//! [`AgentRegistry::load_from_path`] scans `<base>/agents/`
//! for subdirectories containing a `metadata.yaml` +
//! `SYSTEM.md` pair, parses the metadata with serde_yaml,
//! hashes the pair, and inserts the result as an
//! [`AgentDefinition`]. The private
//! [`load_definition_from_dir`] does the per-directory work.

use std::path::Path;

use chrono::Utc;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use synthia_core::Error;

use super::{agent_registry::AgentRegistry, types::AgentDefinition};

impl AgentRegistry {
    /// Walk `<base>/agents/` and load every subdirectory that
    /// contains a `metadata.yaml` + `SYSTEM.md` pair.
    ///
    /// Returns the count of successfully loaded definitions.
    /// Invalid agent dirs are logged at WARN and skipped —
    /// this function never errors on per-directory failures.
    pub fn load_from_path<P: AsRef<Path>>(
        &self,
        base_path: P,
    ) -> Result<usize, Error> {
        let base = base_path.as_ref();
        let agents_dir = base.join("agents");

        if !agents_dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        let entries = std::fs::read_dir(&agents_dir).map_err(|e| {
            Error::Parse(format!("failed to read directory: {}", e))
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                match self.load_definition_from_dir(&path) {
                    Ok(def) => {
                        let mut defs = self.definitions.write();
                        defs.insert(def.id.clone(), def);
                        count += 1;
                    }
                    Err(e) => {
                        tracing::warn!(path = ?path, error = ?e, "Skipping invalid agent definition");
                    }
                }
            }
        }

        Ok(count)
    }

    fn load_definition_from_dir(
        &self,
        dir: &Path,
    ) -> Result<AgentDefinition, Error> {
        let metadata_path = dir.join("metadata.yaml");
        let system_prompt_path = dir.join("SYSTEM.md");

        if !metadata_path.exists() {
            return Err(Error::NotFound(
                "metadata.yaml not found in agent directory".to_string(),
            ));
        }
        if !system_prompt_path.exists() {
            return Err(Error::NotFound(
                "SYSTEM.md not found in agent directory".to_string(),
            ));
        }

        let metadata_content =
            std::fs::read_to_string(&metadata_path).map_err(Error::Io)?;
        let system_prompt =
            std::fs::read_to_string(&system_prompt_path).map_err(Error::Io)?;

        #[derive(Deserialize)]
        struct RawMetadata {
            name: String,
            description: String,
            capabilities: Option<Vec<String>>,
            when_to_use: Option<Vec<String>>,
            constraints: Option<Vec<String>>,
            enabled: Option<bool>,
        }

        let raw: RawMetadata = serde_yaml::from_str(&metadata_content)
            .map_err(|e| {
                Error::Parse(format!("failed to parse metadata.yaml: {}", e))
            })?;

        let name = raw.name;
        let id = name.to_lowercase().replace(' ', "-");

        let mut hasher = Sha256::new();
        hasher.update(metadata_content.as_bytes());
        hasher.update(system_prompt.as_bytes());
        let file_hash = format!("{:x}", hasher.finalize());

        Ok(AgentDefinition {
            id,
            name,
            description: raw.description,
            capabilities: raw.capabilities.unwrap_or_default(),
            when_to_use: raw.when_to_use.unwrap_or_default(),
            constraints: raw.constraints.unwrap_or_default(),
            system_prompt,
            source_path: dir.to_path_buf(),
            file_hash,
            loaded_at: Utc::now(),
            enabled: raw.enabled.unwrap_or(true),
            permission_rules: Vec::new(),
            permission_default: None,
            tools: None,
            denied_tools: None,
            extends: None,
            mode: None,
        })
    }
}
