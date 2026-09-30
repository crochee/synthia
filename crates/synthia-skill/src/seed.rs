//! Seed the workspace with the built-in workflow skills.
//!
//! A "skill" in the MVP is a markdown workflow guide: a `SKILL.md`
//! file with a YAML frontmatter (name + description) and a body
//! describing a procedure the agent can apply when the situation
//! matches. Skills are **not** tool aliases — they are passive
//! guidance that the runtime surfaces to the agent as context.
//!
//! `seed_default_skills` writes the canonical workflow skills to
//! `<workspace>/.agents/skills/<name>/SKILL.md`. The HTTP
//! `GET /api/v1/skills` endpoint reads from this directory, so the
//! seed makes the skills list non-empty on a fresh checkout.
//! Existing user-installed skills are never overwritten.

use std::{fs, path::Path};

const SEED_SKILLS: &[(&str, &str)] = &[
    (
        "code-review",
        "---\n\
         name: code-review\n\
         description: Procedure for reviewing a change before merge.\n\
         ---\n\
         \n\
         # Code review\n\
         \n\
         Apply this workflow when asked to review code or a pull\n\
         request.\n\
         \n\
         **Steps**\n\
         \n\
         1. Identify the change scope from the diff or commit message.\n\
         2. Walk every modified file and note the intent.\n\
         3. Flag correctness, error handling, and test coverage gaps.\n\
         4. Suggest concrete improvements with file + line references.\n\
         5. Summarise blocking issues vs. nits in the final reply.\n",
    ),
    (
        "bug-investigation",
        "---\n\
         name: bug-investigation\n\
         description: Procedure for diagnosing a reported bug or unexpected behaviour.\n\
         ---\n\
         \n\
         # Bug investigation\n\
         \n\
         Apply this workflow when the user reports a defect or asks\n\
         \"why is X happening?\".\n\
         \n\
         **Steps**\n\
         \n\
         1. Restate the observed symptom in your own words.\n\
         2. Reproduce locally with a minimal command or test.\n\
         3. Trace the data flow from input to symptom.\n\
         4. Form and rank hypotheses by likelihood.\n\
         5. Confirm with a targeted experiment; report root cause.\n",
    ),
    (
        "test-planning",
        "---\n\
         name: test-planning\n\
         description: Procedure for designing a test plan for a feature or fix.\n\
         ---\n\
         \n\
         # Test planning\n\
         \n\
         Apply this workflow when designing tests for new behaviour\n\
         or hardening an existing feature.\n\
         \n\
         **Steps**\n\
         \n\
         1. Enumerate the user-visible behaviours under test.\n\
         2. For each, list the happy path and at least one failure mode.\n\
         3. Identify the right test layer (unit, integration, E2E).\n\
         4. Map each case to a concrete assertion or fixture.\n\
         5. Flag residual coverage gaps the plan does not address.\n",
    ),
];

/// Seed `<workspace>/.agents/skills/` with the built-in workflow
/// skills if they do not already exist.
///
/// Returns the number of skills newly written. Existing skills are
/// left untouched so user edits survive server restarts.
pub fn seed_default_skills(workspace_root: &Path) -> Result<usize, String> {
    let skills_dir = workspace_root.join(".agents").join("skills");
    fs::create_dir_all(&skills_dir)
        .map_err(|e| format!("create {}: {e}", skills_dir.display()))?;

    let mut written = 0;
    for (name, content) in SEED_SKILLS {
        let skill_dir = skills_dir.join(name);
        let skill_md = skill_dir.join("SKILL.md");
        if skill_md.exists() {
            continue;
        }
        fs::create_dir_all(&skill_dir)
            .map_err(|e| format!("create {}: {e}", skill_dir.display()))?;
        fs::write(&skill_md, content)
            .map_err(|e| format!("write {}: {e}", skill_md.display()))?;
        written += 1;
        tracing::info!(
            skill = name,
            path = %skill_md.display(),
            "Seeded default workflow skill",
        );
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_writes_three_workflow_skills_into_empty_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let written = seed_default_skills(dir.path()).unwrap();
        assert_eq!(written, 3);
        for (name, _) in SEED_SKILLS {
            let md = dir
                .path()
                .join(".agents")
                .join("skills")
                .join(name)
                .join("SKILL.md");
            assert!(md.exists(), "missing {name}");
        }
    }

    #[test]
    fn seed_is_idempotent_and_does_not_overwrite_existing_skills() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(seed_default_skills(dir.path()).unwrap(), 3);

        let read_file = dir
            .path()
            .join(".agents")
            .join("skills")
            .join("code-review")
            .join("SKILL.md");
        let original = fs::read_to_string(&read_file).unwrap();
        assert_eq!(seed_default_skills(dir.path()).unwrap(), 0);
        let after = fs::read_to_string(&read_file).unwrap();
        assert_eq!(after, original, "existing skill must be preserved");
    }
}
