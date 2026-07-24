use crossterm::style::Stylize;
use synthia_core::generate_session_id;
use synthia_session::store::Store as SessionStore;

use super::super::{
    construct::current_user_id,
    print::print_cli_error,
    types::ReplContext,
};

pub(super) fn handle_session_list(ctx: &mut ReplContext) {
    let sessions_dir = ctx.workspace_root.join(".agents/sessions");
    let store = SessionStore::new(sessions_dir);
    let user_id = match current_user_id() {
        Ok(id) => id,
        Err(e) => {
            print_cli_error(format!("[identity_error] {}", e));
            return;
        }
    };
    match store.list_sessions_with_metadata(&user_id) {
        Ok(sessions) if sessions.is_empty() => {
            println!("No persisted sessions found.");
        }
        Ok(sessions) => {
            println!(
                "{:<16} {:<12} {:<10} {:<8} {:<24}",
                "ID", "State", "Messages", "Model", "Last Updated"
            );
            println!("{}", "-".repeat(78));
            for meta in &sessions {
                let id_display = if meta.id == ctx.session_id {
                    format!("* {}", meta.id).red().to_string()
                } else {
                    meta.id.clone()
                };
                println!(
                    "{:<16} {:<12} {:<10} {:<8} {:<24}",
                    id_display,
                    format!("{:?}", meta.state),
                    meta.message_count,
                    meta.config.model.chars().take(12).collect::<String>(),
                    meta.updated_at.chars().take(19).collect::<String>(),
                );
            }
        }
        Err(e) => {
            print_cli_error(format!("[session_error] {}", e));
        }
    }
}

pub(super) fn handle_session_switch(ctx: &mut ReplContext, id: String) {
    let sessions_dir = ctx.workspace_root.join(".agents/sessions");
    let store = SessionStore::new(sessions_dir);
    let user_id = match current_user_id() {
        Ok(id) => id,
        Err(e) => {
            print_cli_error(format!("[identity_error] {}", e));
            return;
        }
    };
    if !store.session_exists(&user_id, &id) {
        print_cli_error(format!(
            "[not_found] Session '{}' does not exist.",
            id
        ));
    } else {
        ctx.session_id = id.clone();
        println!("Switched to session: {}", id.as_str().green());
    }
}

pub(super) fn handle_session_delete(ctx: &mut ReplContext, id: String) {
    let sessions_dir = ctx.workspace_root.join(".agents/sessions");
    let store = SessionStore::new(sessions_dir);
    let user_id = match current_user_id() {
        Ok(id) => id,
        Err(e) => {
            print_cli_error(format!("[identity_error] {}", e));
            return;
        }
    };
    if !store.session_exists(&user_id, &id) {
        print_cli_error(format!(
            "[not_found] Session '{}' does not exist.",
            id
        ));
    } else {
        match store.delete_session(&user_id, &id) {
            Ok(()) => {
                if ctx.session_id == id {
                    let new_id = generate_session_id();
                    println!(
                        "Session '{}' deleted. Created new session: {}",
                        id.red(),
                        new_id.as_str().green()
                    );
                    ctx.session_id = new_id;
                } else {
                    println!("Session '{}' deleted.", id.red());
                }
            }
            Err(e) => {
                print_cli_error(format!(
                    "[session_error] Failed to delete session '{}': {}",
                    id, e
                ));
            }
        }
    }
}
