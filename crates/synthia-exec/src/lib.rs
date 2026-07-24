// Compatibility shim — `synthia-exec` was split into `synthia-tool-bash` and
// `synthia-tool-exec-base`. New code should depend on the two crates
// directly. This shim exists so existing dependents and external users of
// the public API continue to resolve through the original `synthia_exec::*`
// path.
pub use synthia_tool_bash::*;
pub use synthia_tool_exec_base::*;
