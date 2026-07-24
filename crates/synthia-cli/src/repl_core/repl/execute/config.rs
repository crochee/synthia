use std::sync::Arc;

use crossterm::style::Stylize;
use synthia_provider::config::WorkspaceConfig;

use super::super::{print::print_cli_error, types::ReplContext};

pub(super) fn handle_config_show(ctx: &ReplContext) {
    let cfg = &ctx.workspace_config;
    println!("{}", "=== Workspace Configuration ===".green());
    println!("{:<25} {}", "Default Provider:", cfg.default_provider);
    println!("{:<25} {}", "Default Model:", cfg.default_model);
    println!();
    println!("{}", "=== Configured Providers ===".green());
    println!(
        "{:<15} {:<8} {:<25} {:<15}",
        "Name", "Type", "Default Model", "Context Window"
    );
    println!("{}", "-".repeat(68));
    for (name, entry) in &cfg.providers {
        let indicator = if name == &cfg.default_provider {
            "*".green().to_string()
        } else {
            " ".to_string()
        };
        let model = entry.default_model.as_deref().unwrap_or("none");
        let ctx_window = entry
            .context_window
            .map(|c| c.to_string())
            .unwrap_or_else(|| "unknown".into());
        println!(
            "{:<15} {:<8} {:<25} {:<15}",
            format!("{} {}", indicator, name),
            entry.r#type,
            model,
            ctx_window
        );
    }
}

pub(super) fn handle_config_reload(ctx: &mut ReplContext) {
    match WorkspaceConfig::load_from_dir(&ctx.workspace_root) {
        Ok(new_config) => {
            let old_model = ctx.current_model.clone();
            let old_provider = ctx.current_provider_name.clone();
            ctx.current_model = new_config.default_model.clone();
            ctx.current_provider_name = new_config.default_provider.clone();
            match new_config.create_default_provider() {
                Ok(provider) => {
                    ctx.provider = Some(Arc::from(provider));
                    println!("Configuration reloaded successfully.");
                    if old_model != ctx.current_model
                        || old_provider != ctx.current_provider_name
                    {
                        println!(
                            "  Provider changed: {} -> {}",
                            old_provider.red(),
                            ctx.current_provider_name.as_str().green()
                        );
                        println!(
                            "  Model changed: {} -> {}",
                            old_model.red(),
                            ctx.current_model.as_str().green()
                        );
                    } else {
                        println!("  Provider and model unchanged.");
                    }
                }
                Err(e) => {
                    print_cli_error(format!(
                        "[provider_error] Failed to create default provider: {}",
                        e
                    ));
                }
            }
            ctx.workspace_config = new_config;
        }
        Err(e) => {
            print_cli_error(format!(
                "[internal_server_error] Failed to reload config: {}",
                e
            ));
        }
    }
}

pub(super) fn handle_task_list() {
    println!(
        "No task dispatcher configured. Tasks are managed by the agent's internal ReAct loop."
    );
    println!("Use /session to view the current session state.");
}
