use super::super::types::ReplContext;

pub(super) async fn handle_memory_show(ctx: &ReplContext) {
    if let Some(ref hot) = ctx.hot_memory {
        let memory_content = hot.read_memory().await;
        match memory_content {
            Ok(Some(content)) => {
                println!("=== MEMORY.md ===");
                println!("{}", content);
            }
            Ok(None) => println!("MEMORY.md not found."),
            Err(e) => println!("Error reading MEMORY.md: {}", e),
        }

        let user_content = hot.read_user().await;
        match user_content {
            Ok(Some(content)) => {
                println!("=== USER.md ===");
                println!("{}", content);
            }
            Ok(None) => println!("USER.md not found."),
            Err(e) => println!("Error reading USER.md: {}", e),
        }
    } else {
        println!("HotMemory not initialized.");
    }

    if let Some(ref episodic) = ctx.episodic_memory {
        match episodic.load_all(50).await {
            Ok(skills) if skills.is_empty() => {
                println!("No episodic skills recorded yet.");
            }
            Ok(skills) => {
                println!("=== Episodic Skills ({}) ===", skills.len());
                for skill in skills.iter().take(20) {
                    println!(
                        "  - [{}] {} (rate: {:.1})",
                        skill.used_at.format("%Y-%m-%d %H:%M"),
                        skill.task_hint.chars().take(80).collect::<String>(),
                        skill.success_rate,
                    );
                }
            }
            Err(e) => {
                println!("Error loading episodic skills: {}", e)
            }
        }
    }
}

pub(super) async fn handle_memory_list(ctx: &ReplContext) {
    if let Some(ref hot) = ctx.hot_memory {
        let all = hot.read_all().await;
        match all {
            Ok(entries) if entries.is_empty() => {
                println!("No hot memory entries.");
            }
            Ok(entries) => {
                println!("=== Hot Memory Entries ===");
                for (key, value) in &entries {
                    println!("  [{}] {} chars", key, value.chars().count());
                }
            }
            Err(e) => {
                println!("Error reading hot memory: {}", e)
            }
        }
    }
}

pub(super) async fn handle_memory_read(ctx: &ReplContext, key: &str) {
    if let Some(ref hot) = ctx.hot_memory {
        match hot.read(key).await {
            Ok(Some(content)) => {
                println!("=== {} ===", key);
                println!("{}", content);
            }
            Ok(None) => {
                println!("Key '{}' not found in hot memory.", key)
            }
            Err(e) => {
                println!("Error reading key '{}': {}", key, e)
            }
        }
    }
}

pub(super) async fn handle_memory_set(ctx: &ReplContext, rest: &str) {
    // Task 10.12: memory set subcommand
    if let Some((key, value)) = rest.split_once('=') {
        if let Some(ref hot) = ctx.hot_memory {
            match hot.write(key.trim(), value.trim()).await {
                Ok(()) => {
                    println!("Memory '{}' updated.", key.trim())
                }
                Err(e) => {
                    println!("Error setting memory: {}", e)
                }
            }
        } else {
            println!("HotMemory not initialized.");
        }
    } else {
        println!("Usage: /memory set <key>=<value>");
    }
}
