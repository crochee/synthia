use std::panic;

pub fn init_panic_handler() {
    panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());

        let message = if let Some(s) =
            panic_info.payload().downcast_ref::<&str>()
        {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        tracing::error!(
            location = %location,
            message = %message,
            "Unhandled panic in synthia-agent"
        );

        eprintln!("PANIC at {}: {}", location, message);
    }));
}

pub fn run_with_panic_handler<F, T>(f: F) -> T
where
    F: FnOnce() -> T,
{
    init_panic_handler();
    f()
}
