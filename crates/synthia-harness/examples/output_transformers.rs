//! Output transformers + full-output retrieval, end to end.
//!
//! Seam shown: the commit-path transformer (`synthia_steering::
//! OutputTransformer`) and the retrieval tool are two halves of one
//! contract. A large tool result is *extracted* and *budgeted* before
//! it is committed, and the elided remainder is stashed in a
//! [`FullOutputStore`] under a handle the model can cite; the
//! `__get_full_output` tool reads that handle back when the model
//! asks. Nothing here touches the network.
//!
//! Run:
//!
//! ```text
//! cargo run -p synthia-harness --example output_transformers
//! ```
//!
//! Look at: the committed text ends with a truncation marker naming a
//! handle, and the tool call for that handle returns the full
//! extracted payload — the two halves round-trip through one store.

use std::sync::Arc;

use serde_json::json;
use synthia_context::AgentState;
use synthia_core::{FullOutputStore, InMemoryFullOutputStore};
use synthia_steering::{
    BudgetAwareTruncator,
    JsonExtractor,
    OutputTransformer,
    TransformerChain,
};
use synthia_tool::{Context, FULL_OUTPUT_TOOL_NAME, Tool, full_output_tool};

/// Pull the handle out of a truncation marker.
///
/// The marker is produced by [`BudgetAwareTruncator`]; the example
/// reads it back the way the model would (by citing the handle), so
/// this is the caller-side parse of the published format.
fn handle_in(text: &str) -> Option<String> {
    const NEEDLE: &str = "__get_full_output(\"";
    let start = text.find(NEEDLE)? + NEEDLE.len();
    let rest = text.get(start..)?;
    let end = rest.find('"')?;
    rest.get(..end).map(str::to_owned)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // The store is shared: the transformer writes it, the tool reads
    // it, and both see one another's entries.
    let store: Arc<dyn FullOutputStore> =
        Arc::new(InMemoryFullOutputStore::new(16, 1 << 20));

    let chain = TransformerChain::new(vec![
        Arc::new(JsonExtractor::new()),
        Arc::new(BudgetAwareTruncator::new(Arc::clone(&store), 240)),
    ]);

    // A noisy build log with a JSON payload in the middle, padded so
    // it cannot fit the transformer's budget.
    let payload = json!({
        "status": "ok",
        "files": (0..40).map(|n| format!("src/module_{n}.rs")).collect::<Vec<_>>(),
    })
    .to_string();
    let noisy = format!(
        "compiling 40 crates\nwarning: 3 unused imports\n{payload}\n\
         finished in 12.4s\n{}",
        "x".repeat(400)
    );

    let committed = chain
        .transform(noisy, "shell", &AgentState::with_window(200_000))
        .await;
    println!("--- committed to the conversation ---\n{committed}");

    let Some(handle) = handle_in(&committed) else {
        panic!("the truncator must publish a handle in its marker");
    };

    // The model's next move: cite the handle.
    let tool = full_output_tool(Arc::clone(&store));
    let context = Context::new("example".to_string(), std::env::temp_dir());
    let result = tool.call(json!({ "handle": handle }), &context).await;
    let retrieved: String = result
        .content
        .iter()
        .filter_map(|part| part.text())
        .collect();

    println!(
        "\n--- {} returned {} chars ---",
        FULL_OUTPUT_TOOL_NAME,
        retrieved.len()
    );
    println!(
        "first 120 chars: {}",
        &retrieved[..120.min(retrieved.len())]
    );
    println!(
        "full payload recovered: {}",
        retrieved.contains("module_39.rs")
    );
    println!("store entries: {}", store.len());

    // An unknown handle is a model-facing error, not a panic.
    let missing = tool.call(json!({ "handle": "out-999" }), &context).await;
    println!("unknown handle is_error: {:?}", missing.is_error);

    println!("OUTPUT-TRANSFORMERS: OK");
}
