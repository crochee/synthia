//! Tool-call interception — the harness's plugin point for
//! synthetic tools that are not registry entries.
//!
//! The registry answers "what is registered"; an interceptor
//! answers "what needs the loop itself". A delegation plugin
//! (`synthia-tool-task`'s `task` tool) is the canonical case: it
//! needs the run's event sink to forward child traces, the cancel
//! token to propagate aborts, and the depth bookkeeping that caps
//! nesting — none of which the [`synthia_tool::Tool`] contract
//! carries. The loop consults the installed interceptors *before*
//! registry dispatch, so the model just sees another tool
//! definition in its tool list.
//!
//! The harness stays plugin-free: with zero interceptors installed
//! there is no `task` tool, no delegation code, and no coupling to
//! any multi-agent machinery. Everything beyond the core loop is
//! composed in through this seam (or through the registry).

use std::sync::Arc;

use futures::future::BoxFuture;
use synthia_core::CancelToken;
use synthia_provider::{ToolDefinition, ToolUse};
use synthia_tool::ToolOutput;

use crate::events::AgentEvent;

/// One intercepted tool call, with the loop internals a plugin may
/// need.
pub struct InterceptorCall<'a> {
    /// The model's tool-use request, verbatim (`id`, `name`,
    /// `input`).
    pub call: &'a ToolUse,
    /// The run's cancellation token. Plugins that spawn child work
    /// share it, so an aborted parent aborts the children.
    pub cancel: Arc<dyn CancelToken>,
    /// This loop's sub-agent depth (0 = a top-level run).
    pub depth: usize,
    /// Forward an event into the parent run's stream. Events
    /// forwarded here reach the caller exactly like loop-owned
    /// events.
    pub emit: &'a (dyn Fn(AgentEvent) + Send + Sync),
}

/// A synthetic tool the loop dispatches ahead of the registry.
///
/// Install with
/// [`ReActAgent::with_interceptor`](crate::agent::ReActAgent::with_interceptor).
/// The loop advertises
/// [`definitions`](ToolInterceptor::definitions) alongside the
/// registry's tools (subject to the same tool restriction) and
/// routes any call whose name an interceptor
/// [`claims`](ToolInterceptor::claims) to
/// [`execute`](ToolInterceptor::execute) instead of the registry.
pub trait ToolInterceptor: Send + Sync {
    /// The wire definitions to advertise to the model.
    fn definitions(&self) -> Vec<ToolDefinition>;

    /// Whether this interceptor claims `name`. Checked before
    /// registry dispatch, so a claimed name never reaches the
    /// registry.
    fn claims(&self, name: &str) -> bool;

    /// Execute the claimed call and produce the tool result.
    fn execute<'a>(
        &'a self,
        call: InterceptorCall<'a>,
    ) -> BoxFuture<'a, ToolOutput>;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A minimal interceptor used by the loop's seam tests: one
    /// definition, one claimed name, a canned result.
    pub(crate) struct EchoInterceptor {
        pub name: &'static str,
    }

    impl ToolInterceptor for EchoInterceptor {
        fn definitions(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition {
                name: self.name.to_string(),
                description: "echo the input back".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
                cache_control: None,
                annotations: None,
            }]
        }

        fn claims(&self, name: &str) -> bool {
            name == self.name
        }

        fn execute<'a>(
            &'a self,
            call: InterceptorCall<'a>,
        ) -> BoxFuture<'a, ToolOutput> {
            Box::pin(async move {
                let _ = call.depth;
                ToolOutput::text(format!(
                    "intercepted:{}:{}",
                    self.name, call.call.input
                ))
            })
        }
    }

    #[test]
    fn claims_matches_only_its_own_name() {
        let i = EchoInterceptor { name: "echo_me" };
        assert!(i.claims("echo_me"));
        assert!(!i.claims("shell"));
        assert_eq!(i.definitions().len(), 1);
        assert_eq!(i.definitions()[0].name, "echo_me");
    }
}
