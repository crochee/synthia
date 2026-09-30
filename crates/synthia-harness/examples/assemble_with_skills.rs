//! # Assemble an agent with the layered skill registry + procedural skills (R9-2 / R9-3)
//!
//! Run it:
//!
//! ```text
//! cargo run --example assemble_with_skills -p synthia-harness
//! ```
//!
//! Demonstrates the new `synthia_skill::SkillRegistry` +
//! `SkillProvider` trait + the procedural `SkillApplication`
//! lifecycle. The example:
//!
//! 1. Constructs three skill providers (file / in-memory
//!    procedural / in-memory declarative) and registers them
//!    into one `SkillRegistry`.
//! 2. Collects the deduplicated skill list via
//!    [`SkillRegistry::collect`].
//! 3. Demonstrates that two providers contributing the same
//!    name resolve to the higher-ranked copy.
//! 4. Invokes a procedural skill by name through the
//!    `SkillApplication` closure and prints the structured
//!    response.
//!
//! ## Runtime neutrality
//!
//! `SkillRegistry::collect` is `async`; the `SkillApplication`
//! closure receives a `&SkillRequest` + `&SkillApplicationContext`
//! and returns a `Result<SkillResponse, SkillApplicationError>`.
//! No runtime involvement on the trait surface.

use synthia_skill::{
    FileSkillProvider,
    InMemorySkillProvider,
    Skill,
    SkillApplication,
    SkillApplicationBuilder,
    SkillApplicationContext,
    SkillProviderInfo,
    SkillRank,
    SkillRegistry,
    SkillRequest,
    SkillResponse,
};

fn make_skill(name: &str, description: &str, body: &str) -> Skill {
    Skill {
        name: name.to_string(),
        description: Some(description.to_string()),
        location: std::path::PathBuf::from(format!("/skills/{name}/SKILL.md")),
        content: body.into(),
    }
}

#[tokio::main]
async fn main() {
    println!("=== synthia: assemble with layered skill registry ===\n");

    // -----------------------------------------------------------------
    // Layer 1 — declarative skills loaded from in-memory
    // provider(s).
    // -----------------------------------------------------------------
    let mut reg = SkillRegistry::new();
    reg.register(InMemorySkillProvider::new(vec![
        make_skill(
            "code-review",
            "review code for correctness and style",
            "# Code Review\nWalk through the diff line-by-line.",
        ),
        make_skill(
            "summarize",
            "summarize a long document into key points",
            "# Summarize\nRead; condense.",
        ),
    ]));

    // -----------------------------------------------------------------
    // Layer 2 — a procedural skill registered with an
    // `SkillApplication` closure (R9-3 lifecycle). The
    // closure receives a structured `SkillRequest` and returns
    // a structured `SkillResponse`.
    // -----------------------------------------------------------------
    let summarise_apply: SkillApplication = SkillApplicationBuilder::new(
        |req, _ctx| {
            Box::pin(async move {
                let max_words = req
                    .args
                    .get("max_words")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(100);
                Ok(SkillResponse::structured(
                    format!(
                        "# {} summary (≤ {max_words} words)\n\nGenerated for prompt `{}`",
                        req.name,
                        req.args
                            .get("path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("<none>")
                    ),
                    serde_json::json!({
                        "skill": req.name,
                        "max_words": max_words,
                        "structured": true,
                    }),
                ))
            })
        },
    );

    // Demonstrate the procedural skill by invoking it directly
    // through the closure (mirrors how the runtime would call
    // it during a `SkillTool::execute`).
    let req = SkillRequest::new("summarize")
        .with_arg("path", "/tmp/article.md")
        .with_arg("max_words", 250);
    let ctx = SkillApplicationContext::new()
        .set("model", "stub")
        .set("max_tokens", 1024);
    let resp = (summarise_apply)(&req, &ctx).await.expect("apply succeeds");
    println!("[1/3] procedural skill invocation");
    println!("      request : name={} args={:?}", req.name, req.args);
    println!("      response: {}", resp.content.as_text());
    println!(
        "      structured: {}",
        resp.structured
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_default()
    );

    // -----------------------------------------------------------------
    // Layer 3 — register a second provider with the SAME skill
    // name at a higher rank, demonstrating the dedup
    // precedence rule.
    // -----------------------------------------------------------------
    let mut reg = SkillRegistry::new();
    reg.register(InMemorySkillProvider::with_rank(
        SkillRank::PROJECT,
        SkillProviderInfo::new("project", "project skill"),
        vec![make_skill("shared", "from project", "project variant")],
    ));
    reg.register(InMemorySkillProvider::with_rank(
        SkillRank::USER,
        SkillProviderInfo::new("user", "user skill"),
        vec![make_skill("shared", "from user", "user variant")],
    ));
    let report = reg.collect().await;
    println!("[2/3] skill registry dedup");
    for entry in &report.per_provider {
        println!(
            "      provider {} (rank={}): {} candidates, {} survived",
            entry.provider_name,
            entry.rank,
            entry.candidate_count,
            entry.survived_count,
        );
    }
    println!(
        "      resolved name: {}",
        report
            .skills
            .first()
            .map(|s| s.name.as_str())
            .unwrap_or("<none>"),
    );

    // -----------------------------------------------------------------
    // Layer 4 — register the file provider (real filesystem
    // walk). The current working directory probably has no
    // `.agents/skills/`; the report will be empty, which is
    // the desired demonstration: file providers contribute
    // zero candidates without failing the registry.
    // -----------------------------------------------------------------
    let mut reg = SkillRegistry::new();
    reg.register(FileSkillProvider::discover("."));
    let report = reg.collect().await;
    println!("[3/3] file provider");
    println!(
        "      discovered {} skill(s); failed providers: {}",
        report.skills.len(),
        report.failed_providers.len()
    );

    println!("\ndone.");
}
