use super::super::types::ReplContext;

pub(super) fn handle_model(model: Option<String>, ctx: &mut ReplContext) {
    match model {
        None => {
            println!("Current model: {}", ctx.current_model);
        }
        Some(model) => {
            ctx.current_model = model.clone();
            println!("Model switched to: {}", model);
        }
    }
}
