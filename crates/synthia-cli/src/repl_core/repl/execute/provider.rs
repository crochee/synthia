use std::sync::Arc;

use super::super::types::ReplContext;

pub(super) fn handle_provider(name: Option<String>, ctx: &mut ReplContext) {
    match name {
        None => {
            println!("Current provider: {}", ctx.current_provider_name);
        }
        Some(name) => {
            if ctx.workspace_config.providers.contains_key(&name) {
                match ctx.workspace_config.create_provider(&name) {
                    Ok(p) => {
                        ctx.provider = Some(Arc::from(p));
                        ctx.current_provider_name = name.clone();
                        if let Some(entry) =
                            ctx.workspace_config.providers.get(&name)
                            && let Some(model) = &entry.default_model
                        {
                            ctx.current_model = model.clone();
                        }
                        println!(
                            "Provider switched to: {} (model: {})",
                            name, ctx.current_model
                        );
                    }
                    Err(e) => {
                        println!("Failed to create provider '{}': {}", name, e);
                    }
                }
            } else {
                println!(
                    "Unknown provider: {}. Available: {:?}",
                    name,
                    ctx.workspace_config.available_providers()
                );
            }
        }
    }
}
