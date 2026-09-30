//! Test helpers for draining a `ToolRegistry::run_stream` stream.
//!
//! Used by integration tests that don't care about `Progress` items
//! and just want to collect the final `Result` outputs.

use synthia_tool::{StreamOutput, ToolOutput};

/// Drain a `run_stream` stream and collect exactly one `Result` per
/// expected call. Drops `Progress` items. Used by tests that don't care
/// about progress visibility — they just want the final outputs.
///
/// # Arguments
///
/// * `stream` — a `run_stream` stream of `(call_id, StreamOutput)` items.
///   Must be `Unpin` (boxed streams satisfy this).
/// * `expected` — number of `Result` items to collect before returning.
///   Once collected, the stream is dropped (cancellation is implicit).
///
/// # Returns
///
/// `Vec<(call_id, ToolOutput)>` — one entry per `Result` yielded by the
/// underlying tool stream, in arrival order. `call_id` matches the
/// `ToolUse::id` from the original `run_stream` call.
pub async fn collect_results(
    stream: impl futures::Stream<Item = (String, StreamOutput)> + Unpin,
    expected: usize,
) -> Vec<(String, ToolOutput)> {
    use futures::StreamExt;
    let mut stream = std::pin::pin!(stream);
    let mut out = Vec::new();
    while let Some((call_id, item)) = stream.next().await {
        if let StreamOutput::Result(output) = item {
            out.push((call_id, output));
            if out.len() == expected {
                break;
            }
        }
    }
    out
}
