use std::{collections::HashMap, path::Path};

use crossterm::style::Stylize;
use synthia_skill::types::SkillSource;

use super::super::types::ReplContext;

pub(super) fn handle_skills() {
    println!("Skills commands are available when skills are configured.");
}

pub(super) fn handle_skill_report(ctx: &ReplContext) {
    let agents_dir = ctx.workspace_root.join(".agents");
    let skills_dir = agents_dir.join("skills");
    let builtin_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills");

    println!("{}", "=== Skill Diagnostic Report ===".green());
    println!();

    let mut total_skills = 0;
    let mut sources: HashMap<String, usize> = HashMap::new();

    let mut scan_dir = |dir: &Path, source: SkillSource| {
        if !dir.exists() {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let skill_md = entry.path().join("SKILL.md");
                if skill_md.exists()
                    && let Ok(meta) =
                        synthia_skill::loader::SkillLoader::parse_frontmatter(
                            &skill_md,
                        )
                {
                    let source_key = match source {
                        SkillSource::BuiltIn => "builtin".to_string(),
                        SkillSource::Project => "project".to_string(),
                        SkillSource::User => "user".to_string(),
                    };
                    *sources.entry(source_key).or_insert(0) += 1;
                    total_skills += 1;
                    println!(
                        "  {:<25} v{}  priority={}  tags=[{}]",
                        meta.name,
                        meta.version.as_deref().unwrap_or("unknown"),
                        meta.priority,
                        meta.tags.join(", ")
                    );
                    println!(
                        "    {}",
                        meta.description.chars().take(80).collect::<String>()
                    );
                    println!("    triggers: {}", meta.triggers.join(", "));
                    println!();
                }
            }
        }
    };

    if builtin_dir.exists() {
        scan_dir(&builtin_dir, SkillSource::BuiltIn);
    }
    if let Ok(dir) = ctx.workspace_root.join(".synthia/skills").canonicalize()
        && dir.exists()
    {
        scan_dir(&dir, SkillSource::Project);
    }
    if skills_dir.exists() {
        scan_dir(&skills_dir, SkillSource::User);
    }

    println!("{}", "--- Summary ---".green());
    println!("Total skills: {}", total_skills);
    for (source, count) in &sources {
        println!("  {}: {}", source, count);
    }
}

pub(super) fn handle_skill_stats(ctx: &ReplContext) {
    let agents_dir = ctx.workspace_root.join(".agents");
    let skills_dir = agents_dir.join("skills");
    let builtin_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills");
    let project_dir = ctx.workspace_root.join(".synthia/skills");

    println!("{}", "=== Skill System Statistics ===".green());

    let scan = |dir: &Path| -> usize {
        let mut count = 0;
        if !dir.exists() {
            return 0;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let skill_md = entry.path().join("SKILL.md");
                if skill_md.exists()
                    && synthia_skill::loader::SkillLoader::parse_frontmatter(
                        &skill_md,
                    )
                    .is_ok()
                {
                    count += 1;
                }
            }
        }
        count
    };

    let builtin = scan(&builtin_dir);
    let project = if project_dir.exists() {
        scan(&project_dir)
    } else {
        0
    };
    let user = if skills_dir.exists() {
        scan(&skills_dir)
    } else {
        0
    };
    let total = builtin + project + user;

    println!("{:<25} {}", "Total skills:", total);
    println!("{:<25} {}", "  Built-in:", builtin);
    println!("{:<25} {}", "  Project:", project);
    println!("{:<25} {}", "  User:", user);
    println!(
        "{:<25} {}",
        "Matching tier:",
        if total < 20 {
            "Keyword"
        } else if total <= 100 {
            "BM25"
        } else {
            "Hybrid (BM25 + Vector)"
        }
    );
    println!(
        "{:<25} {}",
        "BM25 index status:",
        if total >= 20 {
            "Built"
        } else {
            "Not required (< 20 skills)"
        }
    );
    println!(
        "{:<25} {}",
        "Vector index status:",
        if total > 100 {
            "Built"
        } else {
            "Not required (<= 100 skills)"
        }
    );

    let skill_count = ctx
        .skill_summaries
        .as_ref()
        .map(|s| s.lines().count())
        .unwrap_or(0);
    println!("{:<25} {}", "Loaded skill summaries:", skill_count);
}
