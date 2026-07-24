/// Trait for pluggable context data sources.
///
/// Implementors can inject system prompts and memories into the context assembly
/// process. Each injector is identified by name and can provide both system-level
/// instructions and historical memory entries.
pub trait ContextInjector: Send + Sync {
    /// Returns the name of this injector, used for identification and logging.
    fn name(&self) -> &str;

    /// Returns an optional system prompt to inject.
    ///
    /// If `Some`, the returned string will be included in the system prompt
    /// section during context assembly.
    fn inject_system_prompt(&self) -> Option<String>;

    /// Returns a list of memory entries to inject.
    ///
    /// Each entry is a (title, content) pair. These will be formatted as
    /// memory sections during context assembly.
    fn inject_memories(&self) -> Vec<(String, String)>;
}
