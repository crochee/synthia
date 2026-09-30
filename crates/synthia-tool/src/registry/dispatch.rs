//! Dispatching a batch of tool uses as one stream, and the
//! per-tool drain that turns a `Tool::stream` into exactly one
//! result. Method continuation of
//! [`ToolRegistry`](super::ToolRegistry).

use std::{panic::AssertUnwindSafe, sync::Arc};

use futures::{FutureExt, Stream, StreamExt};
use tokio::sync::{Semaphore, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use super::ToolRegistry;
use crate::{
    traits::{StreamOutput, Tool},
    types::{Context, ToolOutput},
};

impl ToolRegistry {
    /// Dispatch a batch of tool uses as a stream of `(call_id,
    /// StreamOutput)` items. Each input `ToolUse` yields zero-or-more
    /// `Progress` items then exactly one `Result`; if the tool's
    /// underlying stream closes without a `Result`, this method
    /// synthesizes an error `Result`. Concurrency is bounded by
    /// `self.max_concurrent`. Cancellation is implicit: dropping the
    /// returned stream aborts all in-flight tasks.
    pub fn run_stream(
        &self,
        tool_uses: Vec<synthia_provider::ToolUse>,
        context: Context,
    ) -> impl Stream<Item = (String, StreamOutput)> + Send + 'static {
        let max_concurrent = self.max_concurrent.max(1);
        let semaphore = Arc::new(Semaphore::new(max_concurrent));

        // Phase 1: resolve every tool_use entry under the lock; build a
        // plan; release the lock before any await point.
        #[allow(clippy::large_enum_variant)]
        enum Plan {
            Call(Arc<dyn Tool>, serde_json::Value),
            NotFound(String),
            /// The call's input failed JSON-Schema validation against
            /// the tool's `parameters()`. The dispatcher synthesises
            /// an `is_error` `Result` listing the dotted-path
            /// violations so the model can self-correct in the same
            /// turn.
            Invalid {
                name: String,
                violations: Vec<synthia_core::SchemaViolation>,
            },
        }
        let validate_arguments = self.validate_arguments;
        let plan: Vec<(String, Plan)> = {
            let inner = self.inner.read();
            tool_uses
                .into_iter()
                .map(|tu| {
                    let key = tu.name.clone();
                    match inner.tools.get(&key).and_then(|e| e.last()) {
                        // `is_hidden` is the registry's privacy flag:
                        // a hidden tool is never advertised and is
                        // refused here. `ToolExposure::Hidden` is the
                        // softer, advertisement-only level and stays
                        // dispatcheable.
                        Some(entry) if entry.is_hidden => {
                            (tu.id, Plan::NotFound(tu.name))
                        }
                        Some(entry) => {
                            // Optional pre-dispatch validation. The
                            // cost is one schema walk per call, paid
                            // under the same read lock as the tool
                            // lookup so a misbehaving call costs a
                            // synthesized error Result instead of a
                            // spawned task that runs a tool it
                            // shouldn't.
                            if validate_arguments
                                && let Err(violations) =
                                    synthia_core::validate_against_schema(
                                        &entry.tool.parameters(),
                                        &tu.input,
                                    )
                            {
                                return (
                                    tu.id,
                                    Plan::Invalid {
                                        name: tu.name,
                                        violations,
                                    },
                                );
                            }
                            (tu.id, Plan::Call(entry.tool.clone(), tu.input))
                        }
                        None => (tu.id, Plan::NotFound(tu.name)),
                    }
                })
                .collect()
        };

        // Phase 2: per plan item, create a channel + spawn a task.
        let mut per_tool_streams = Vec::with_capacity(plan.len());
        for (call_id, item) in plan {
            let (tx, rx) = mpsc::channel::<StreamOutput>(16);
            let ctx = context.clone();
            let semaphore = semaphore.clone();
            self.spawner.spawn(Box::pin(async move {
                let _permit = match semaphore.acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => return, // semaphore closed
                };
                match item {
                    Plan::Call(tool, input) => {
                        let tool_name = tool.name();
                        let span = tracing::info_span!(
                            "tool.execute",
                            tool.name = %tool_name,
                            exception.type = tracing::field::Empty,
                            exception.message = tracing::field::Empty,
                            otel.status_code = tracing::field::Empty,
                        );
                        // Panic recovery lives inside
                        // `consume_tool_stream_into` so the Sender —
                        // moved into the helper — stays accessible for
                        // the synthesized error Result. By the time
                        // control returns here, the per-tool task is
                        // finished.
                        consume_tool_stream_into(tool, input, &ctx, tx, span)
                            .await;
                    }
                    Plan::NotFound(name) => {
                        let _ = tx
                            .send(StreamOutput::Result(ToolOutput::error(
                                format!("tool not found: {}", name),
                            )))
                            .await;
                    }
                    Plan::Invalid { name, violations } => {
                        // Format the violations as a numbered list so
                        // the model gets every field it needs to fix
                        // in one read. An empty `violations` is
                        // unreachable in practice
                        // (`validate_against_schema` returns
                        // `Err(vec)` only on failure), but we handle
                        // it defensively to keep the error path
                        // total.
                        let body = if violations.is_empty() {
                            format!("invalid arguments for tool `{name}`")
                        } else {
                            let lines: Vec<String> = violations
                                .iter()
                                .map(|v| format!("- {v}"))
                                .collect();
                            format!(
                                "invalid arguments for tool `{name}` ({}                                  violation{}):
{}",
                                violations.len(),
                                if violations.len() == 1 { "" } else { "s" },
                                lines.join("
"),
                            )
                        };
                        let _ = tx
                            .send(StreamOutput::Result(ToolOutput::error(body)))
                            .await;
                    }
                }
                // tx drops here, closing rx for this tool.
            }));
            let s = ReceiverStream::new(rx).map({
                let call_id = call_id.clone();
                move |item| (call_id.clone(), item)
            });
            per_tool_streams.push(s);
        }

        futures::stream::select_all(per_tool_streams)
    }
}

/// Outcome of a stream-drain attempt (used internally).
enum StreamOutcome {
    /// The tool's `Tool::stream` closed without panicking or being
    /// cancelled.
    Completed,
    /// The consumer dropped the returned stream (or the per-tool
    /// Receiver). The task exits without synthesizing a Result.
    ConsumerDropped,
}

/// Drain a tool's `Tool::stream`, forward items to `tx`, record the
/// outcome on `span`, and synthesize a `Result` if the stream closes
/// without one OR if the underlying future panics. The `tx` is held
/// inside an `AssertUnwindSafe` wrapper because panicking out of an
async fn consume_tool_stream_into(
    tool: Arc<dyn Tool>,
    input: serde_json::Value,
    ctx: &Context,
    tx: mpsc::Sender<StreamOutput>,
    span: tracing::Span,
) {
    let tool_name = tool.name();
    let started = std::time::Instant::now();
    let stream = tool.stream(input, ctx);
    let mut stream = std::pin::pin!(stream);
    let tx = AssertUnwindSafe(tx);

    // Tracks the most recent Result for span recording.
    let mut last_result: Option<ToolOutput> = None;

    let drain_result = AssertUnwindSafe(async {
        while let Some(item) = stream.next().await {
            if let StreamOutput::Result(output) = &item {
                last_result = Some(output.clone());
            }
            // tx.0 is the inner Sender; dereference through AssertUnwindSafe.
            if tx.0.send(item).await.is_err() {
                return StreamOutcome::ConsumerDropped;
            }
        }
        StreamOutcome::Completed
    })
    .catch_unwind()
    .await;

    let outcome = match drain_result {
        Ok(o) => o,
        Err(payload) => {
            // Tool panicked. Record on the span, synthesize an error
            // Result, and forward to the consumer.
            let msg = synthia_core::panic_message(payload);
            let out = ToolOutput::error(format!(
                "tool `{}` panicked during execution: {}",
                tool.name(),
                msg
            ));
            record_tool_outcome(&span, &out);
            synthia_telemetry::record_tool_panic(tool.name());
            synthia_telemetry::record_tool_duration(
                tool.name(),
                started.elapsed().as_secs_f64(),
            );
            let _ = tx.0.send(StreamOutput::Result(out)).await;
            return;
        }
    };

    match outcome {
        StreamOutcome::ConsumerDropped => (), // tx already closed
        StreamOutcome::Completed => {
            if let Some(out) = last_result {
                record_tool_outcome(&span, &out);
                synthia_telemetry::record_tool_outcome_metric(
                    tool.name(),
                    out.is_error.unwrap_or(false),
                );
            } else {
                // Tool stream closed without a Result — synthesize.
                let out = ToolOutput::error(format!(
                    "tool `{}` stream yielded no Result — contract violation",
                    tool.name()
                ));
                record_tool_outcome(&span, &out);
                synthia_telemetry::record_tool_outcome_metric(
                    tool.name(),
                    true,
                );
                let _ = tx.0.send(StreamOutput::Result(out)).await;
            }
            synthia_telemetry::record_tool_duration(
                tool.name(),
                started.elapsed().as_secs_f64(),
            );
        }
    }
    let _ = tool_name; // suppress unused-variable warning
}

/// Record the tool outcome into the `tool.execute` span.
///
/// Mirrors the OTel semantic conventions expected by
/// `crates/synthia-tool/tests/tool_span.rs`:
/// - On success: leaves `exception.*` and `otel.status_code` empty.
/// - On error: classifies by output message:
///   - `"timed out"` → `exception.type = "TimeoutError"`
///   - anything else → `exception.type = "ToolError"`
///
///   and records `otel.status_code = "ERROR"`.
fn record_tool_outcome(span: &tracing::Span, out: &ToolOutput) {
    let is_err = out.is_error.unwrap_or(false);
    if !is_err {
        return;
    }
    let message = out
        .content
        .iter()
        .find_map(|c| match c {
            synthia_provider::types::ContentPart::Text(t) => {
                Some(t.text.clone())
            }
            _ => None,
        })
        .unwrap_or_default();
    let exception_type = if message.to_lowercase().contains("timed out") {
        "TimeoutError"
    } else {
        "ToolError"
    };
    span.record("exception.type", exception_type);
    span.record("exception.message", message.as_str());
    span.record("otel.status_code", "ERROR");
}
