//! Hot-path benchmarks — the work a turn repeats, measured.
//!
//! ```bash
//! make bench            # print the table
//! make bench-check      # enforce the ratios (CI)
//! cargo bench -p synthia-harness --bench hot_paths -- --check
//! ```
//!
//! Two modes. The default prints the table — compare an entry before and
//! after a change, on the same machine. `--check` enforces the **ratios**
//! that the table's absolute times cannot: every optimised path must stay
//! measurably faster than the code it replaced, both measured in the same
//! process. Ratios survive a loaded or different CI runner where absolute
//! nanoseconds do not, so those are the numbers a gate can be built on.
//!
//! The catalog memos (R43, R51) are compared against the expression they
//! replaced, spelled out inline. Two older wins — R42's ASCII fast path
//! and R48's token-unit accumulator — replaced code that is no longer in
//! the tree, so faithful copies of the old algorithms live in this file
//! (see "Reference implementations"): without them the gate could not tell
//! whether those optimisations are still there.
//!
//! Every entry is a function the agent loop (or the provider/tool layer
//! it calls) runs *per iteration* or *per streamed delta*, so a
//! regression here is a regression in every turn. The harness is
//! deliberately dependency-free (`harness = false`, no criterion): the
//! number that matters is the shape of the curve — allocation-heavy
//! paths scale badly with message count, and a wrapper library would
//! only tell us the same thing with a bigger dependency tree.
//!
//! Fixtures are built once, outside the measured loops, and
//! `std::hint::black_box` keeps the optimiser from folding a bench into
//! nothing. Each entry runs several passes and reports the **minimum**
//! per-op time (the estimator that ignores scheduler interference), so
//! compare a number against the same machine before the change, not
//! across machines.

use std::{
    hint::black_box,
    sync::Arc,
    time::{Duration, Instant},
};

use synthia_context::{AgentState, ContextManager, TruncatingContextManager};
use synthia_core::{
    cap_to_char_boundary,
    schema::validate_against_schema,
    token::estimate_token_count,
};
use synthia_provider::{
    BlockAssembler,
    Content,
    ContentPart,
    Message,
    Role,
    StreamChunk,
    TextContent,
    ToolResult,
    estimate_messages_token_count,
    json_repair::{parse_tool_input, repair_json},
};
use synthia_tool::{
    ToolCategory,
    ToolDescriptor,
    ToolExposure,
    called_tool_names,
    project_tool_definitions,
};

/// One measured entry: total time over `iters` runs, reported per op.
struct Bench {
    label: &'static str,
    iters: u32,
}

impl Bench {
    fn run(self, note: &str, f: impl FnMut()) {
        let best = measure(self.iters, f);
        report(self.label, self.iters, best, note);
    }
}

/// Time `iters` runs of `f`, keeping the **minimum** of several passes.
///
/// This machine is shared, so a single pass measures the scheduler as
/// much as the code. The minimum is the standard estimator for "how fast
/// can this run when nothing interferes" — compare it before and after a
/// change, never across machines. `--check` compares two measurements
/// taken here, in one process, which is what makes its ratios portable.
fn measure(iters: u32, mut f: impl FnMut()) -> Duration {
    // Warmup: first-touch page faults and branch predictor state belong
    // to the harness, not to the measurement.
    let warmup = (iters / 20).clamp(1, 200);
    for _ in 0..warmup {
        f();
    }

    const REPEATS: u32 = 5;
    let mut best = Duration::MAX;
    for _ in 0..REPEATS {
        let start = Instant::now();
        for _ in 0..iters {
            f();
        }
        best = best.min(start.elapsed());
    }
    best
}

/// One enforceable performance invariant: `fast` must beat `slow` by at
/// least `min_factor`. Both are measured in this process, so the *factor*
/// is comparable across machines even though the nanoseconds are not.
fn ratio(
    label: &str,
    fast_iters: u32,
    fast: impl FnMut(),
    slow_iters: u32,
    slow: impl FnMut(),
    min_factor: f64,
) -> bool {
    let fast_per =
        measure(fast_iters, fast).as_secs_f64() / f64::from(fast_iters);
    let slow_per =
        measure(slow_iters, slow).as_secs_f64() / f64::from(slow_iters);
    let factor = slow_per / fast_per;
    let held = factor >= min_factor;
    println!(
        "{} {:<54}{factor:>7.1}x  (min {min_factor:>5.1}x; {:.1} ns vs {:.0} ns)",
        if held { "OK  " } else { "FAIL" },
        label,
        fast_per * 1e9,
        slow_per * 1e9,
    );
    held
}

// ---------------------------------------------------------------------
// Reference implementations
//
// Two of the framework's hot paths are only *known* to be fast relative
// to what they replaced, and the replacement is no longer in the tree.
// Keeping a faithful copy of the old code here is what turns "R42 made
// this 200× faster" from a claim in a report into an invariant `--check`
// can enforce: if either optimisation is ever dropped, its factor
// collapses and the gate fails.
//
// They are deliberately the *old* algorithms, kept bit-compatible with
// the current ones' results (the current code pins that equivalence in
// its own tests: `token_units_accumulate_to_the_same_estimate`).
// ---------------------------------------------------------------------

/// The pre-R42 estimator: a per-character walk with the CJK range check
/// and no `str::is_ascii` fast path.
fn slow_estimate_token_count(text: &str) -> usize {
    fn is_cjk(ch: char) -> bool {
        matches!(ch,
            '\u{4E00}'..='\u{9FFF}' |
            '\u{3400}'..='\u{4DBF}' |
            '\u{20000}'..='\u{2A6DF}' |
            '\u{2A700}'..='\u{2B73F}' |
            '\u{2B740}'..='\u{2B81F}' |
            '\u{2B820}'..='\u{2CEAF}' |
            '\u{F900}'..='\u{FAFF}' |
            '\u{2F800}'..='\u{2FA1F}' |
            '\u{3000}'..='\u{303F}' |
            '\u{3040}'..='\u{309F}' |
            '\u{30A0}'..='\u{30FF}' |
            '\u{AC00}'..='\u{D7AF}'
        )
    }

    let mut ascii_bytes = 0usize;
    let mut cjk_chars = 0usize;
    for ch in text.chars() {
        if is_cjk(ch) {
            cjk_chars += 1;
        } else {
            ascii_bytes += ch.len_utf8();
        }
    }
    let text_tokens =
        (ascii_bytes as f64 / 4.0 + cjk_chars as f64 / 1.5) as usize;
    text_tokens + (text_tokens as f64 * 0.05) as usize
}

/// The pre-R48 message estimator: build one `String` per message and
/// measure that, instead of folding `TokenUnits`.
///
/// It calls the *current* [`estimate_token_count`] (i.e. the post-R42
/// one) on purpose: the pair must isolate the accumulator change, not
/// re-measure R42's fast path inside R48's comparison.
fn slow_estimate_messages_token_count(messages: &[Message]) -> usize {
    use synthia_provider::ContentPart;

    messages
        .iter()
        .map(|message| {
            let mut buffer = String::new();
            for part in message.content.iter() {
                match part {
                    ContentPart::Text(tc) => buffer.push_str(&tc.text),
                    ContentPart::Reasoning(rc) => buffer.push_str(&rc.text),
                    ContentPart::ToolResult(tr) => {
                        for inner in &tr.content {
                            if let ContentPart::Text(tc) = inner {
                                buffer.push_str(&tc.text);
                            }
                        }
                    }
                    ContentPart::ToolUse(tu) => {
                        buffer.push_str(&tu.name);
                        if let Ok(json) = serde_json::to_string(&tu.input) {
                            buffer.push_str(&json);
                        }
                    }
                    _ => {}
                }
            }
            estimate_token_count(&buffer)
        })
        .sum()
}

/// The ratios that stand in for "the memo is still there, and still
/// worth it". Each pair is *the same question answered twice* — the
/// memoised path against the expression it replaced — so a regression in
/// either direction (a cache that stopped hitting, or a rewrite that
/// reintroduced the scan) shows up as a failed factor rather than as a
/// number nobody compares.
fn check_ratios() -> bool {
    let registry = tool_registry(40);
    let grouped = {
        let grouped =
            synthia_tool::GroupedRegistry::new(Arc::new(tool_registry(40)));
        let names: Vec<String> =
            (1..=20).map(|i| format!("tool_{i}")).collect();
        let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
        grouped.declare("core", &borrowed).unwrap();
        grouped.activate("core");
        grouped
    };
    let adaptive = synthia_tool::AdaptiveRegistry::new(
        Arc::new(tool_registry(40)),
        synthia_provider::ModelTier::Small,
    );
    let caps = adaptive.limits().max_visible_tools;

    let mut ok = true;
    ok &= ratio(
        "descriptors_cached vs descriptors",
        2_000_000,
        || {
            black_box(registry.descriptors_cached());
        },
        5_000,
        || {
            black_box(registry.descriptors());
        },
        100.0,
    );
    ok &= ratio(
        "snapshot_cached vs snapshot",
        2_000_000,
        || {
            black_box(registry.snapshot_cached());
        },
        10_000,
        || {
            black_box(registry.snapshot());
        },
        100.0,
    );
    ok &= ratio(
        "AdaptiveRegistry::visible_tool_names vs the uncached scan",
        200_000,
        || {
            black_box(adaptive.visible_tool_names());
        },
        10_000,
        || {
            let names: Vec<String> = adaptive
                .inner()
                .snapshot()
                .into_iter()
                .take(caps)
                .map(|snap| snap.name)
                .collect();
            black_box(names);
        },
        10.0,
    );
    // R42's ASCII fast path and R48's unit accumulator, each against the
    // algorithm it replaced (kept above). The keep-alive string is not
    // pinned by the checker; what matters is that the fast path stays
    // overwhelmingly faster than the walk it removed.
    let ascii_4k = "the quick brown fox jumps over the lazy dog. ".repeat(96);
    ok &= ratio(
        "estimate_token_count (ASCII) vs the pre-R42 char walk",
        200_000,
        || {
            black_box(estimate_token_count(black_box(&ascii_4k)));
        },
        100,
        || {
            black_box(slow_estimate_token_count(black_box(&ascii_4k)));
        },
        50.0,
    );
    let history = tool_heavy_history(100);
    ok &= ratio(
        "estimate_messages_token_count vs the pre-R48 String-per-message",
        20_000,
        || {
            black_box(estimate_messages_token_count(black_box(&history)));
        },
        10_000,
        || {
            black_box(slow_estimate_messages_token_count(black_box(&history)));
        },
        // The floor is deliberately close to 1: dropping the accumulator
        // lands *at* 1.0, and the observed factor moves 1.9-6.3x with
        // machine state, so a higher floor would fail on a busy runner
        // without catching anything the 1.3 floor misses.
        1.3,
    );
    ok &= ratio(
        "GroupedRegistry::visible_tool_names vs the uncached scan",
        200_000,
        || {
            black_box(grouped.visible_tool_names());
        },
        10_000,
        || {
            let names: Vec<String> = grouped
                .inner()
                .snapshot()
                .into_iter()
                .filter(|snap| grouped.is_visible(&snap.name))
                .map(|snap| snap.name)
                .collect();
            black_box(names);
        },
        // The smallest margin of the four (the group filter still clones
        // the advertised names), so the floor leaves CI headroom: a
        // regression that removes the memo lands near 1x, far below this.
        2.0,
    );
    ok
}

/// `--check`: enforce the ratios instead of printing the table. Exits
/// non-zero on the first failed invariant, so CI reports it as a gate.
fn main_check() {
    println!(
        "\nperformance invariants — each pair measured in this process, \n\
         so the factor is comparable across machines\n"
    );
    println!("{:<5}{:<54}{:>9}", "", "invariant", "measured");
    println!("{}", "-".repeat(96));
    if !check_ratios() {
        eprintln!(
            "\nFAIL: a memoised path lost its advantage. Re-run `make bench`\n\
             for the full table; if the change is intentional, adjust the\n\
             minimum factor in benches/hot_paths.rs and say why in the commit."
        );
        std::process::exit(1);
    }
    println!("\nall performance invariants hold");
}

fn report(label: &str, iters: u32, elapsed: Duration, note: &str) {
    let per_op = elapsed.as_secs_f64() / f64::from(iters);
    let ns = per_op * 1e9;
    let ops = if per_op > 0.0 { 1.0 / per_op } else { 0.0 };
    println!("{label:<46}{ns:>10.1} ns{ops:>14.0} /s   {note}");
}

fn main() {
    if std::env::args().any(|arg| arg == "--check") {
        main_check();
        return;
    }

    let text_4k = "the quick brown fox jumps over the lazy dog. ".repeat(96);
    let text_200k = "abcdefghij".repeat(20_000);

    // ---- fixtures ---------------------------------------------------
    let messages = tool_heavy_history(100);
    let descriptors = tool_descriptors(40);
    let visible: std::collections::HashSet<String> = descriptors
        .iter()
        .filter(|d| d.exposure != ToolExposure::Hidden)
        .map(|d| d.name.clone())
        .collect();
    let called: std::collections::HashSet<String> = descriptors
        .iter()
        .filter(|d| d.exposure == ToolExposure::Deferred)
        .take(2)
        .map(|d| d.name.clone())
        .collect();

    let schema = serde_json::json!({
        "type": "object",
        "properties": (0..20).map(|i| (format!("field_{i}"), serde_json::json!({ "type": "string" }))).collect::<serde_json::Map<_, _>>(),
        "required": (0..10).map(|i| format!("field_{i}")).collect::<Vec<_>>(),
    });
    let valid_value = serde_json::Value::Object(
        (0..20)
            .map(|i| {
                (
                    format!("field_{i}"),
                    serde_json::Value::String("value".to_string()),
                )
            })
            .collect(),
    );
    let strict_json = nested_tool_args(120);
    let defective_json = format!("{} trailing", strict_json);

    println!("\n{:<46}{:>13}{:>16}", "benchmark", "per op", "throughput");
    println!("{}", "-".repeat(96));

    // ---- token accounting -------------------------------------------
    Bench {
        label: "estimate_token_count (4 KB ascii)",
        iters: 200_000,
    }
    .run("core heuristic", || {
        black_box(estimate_token_count(black_box(&text_4k)));
    });

    Bench {
        label: "estimate_messages_token_count (100 msgs)",
        iters: 20_000,
    }
    .run("compaction gate", || {
        black_box(estimate_messages_token_count(black_box(&messages)));
    });

    // ---- streaming assembly -----------------------------------------
    Bench {
        label: "BlockAssembler fold (400 deltas)",
        iters: 20_000,
    }
    .run("per-delta cost", || {
        let mut assembler = BlockAssembler::new();
        for i in 0..400 {
            assembler.push(StreamChunk::Content(ContentPart::Text(
                TextContent {
                    text: format!("delta-{i} "),
                    cache_control: None,
                },
            )));
        }
        black_box(assembler.finalize_assistant());
    });

    // ---- wire repair ------------------------------------------------
    Bench {
        label: "parse_tool_input (4 KB strict)",
        iters: 50_000,
    }
    .run("happy path", || {
        black_box(parse_tool_input(black_box(&strict_json)));
    });

    Bench {
        label: "repair_json (4 KB defective)",
        iters: 20_000,
    }
    .run("salvage path", || {
        black_box(repair_json(black_box(&defective_json)));
    });

    // ---- schema validation ------------------------------------------
    Bench {
        label: "validate_against_schema (20 fields)",
        iters: 100_000,
    }
    .run("structured output", || {
        black_box(validate_against_schema(
            black_box(&schema),
            black_box(&valid_value),
        ))
        .ok();
    });

    // ---- tool-surface projection ------------------------------------
    Bench {
        label: "project_tool_definitions (40 tools)",
        iters: 50_000,
    }
    .run("per request", || {
        black_box(project_tool_definitions(
            black_box(&descriptors),
            black_box(&called),
            Some(black_box(&visible)),
        ));
    });

    // ---- the per-iteration prep the loop does before caching -------
    Bench {
        label: "called_tool_names (200-message transcript)",
        iters: 20_000,
    }
    .run("deferred promotion", || {
        black_box(called_tool_names(black_box(&messages)));
    });

    let registry = tool_registry(40);
    Bench {
        label: "ToolRegistry::descriptors (40 tools)",
        iters: 20_000,
    }
    .run("per iteration, uncached", || {
        black_box(black_box(&registry).descriptors());
    });

    // Warm the version-keyed memo, then measure the loop's accessor.
    let _ = registry.descriptors_cached();
    Bench {
        label: "ToolRegistry::descriptors_cached (40 tools)",
        iters: 200_000,
    }
    .run("per iteration, warm", || {
        black_box(black_box(&registry).descriptors_cached());
    });

    // ---- the visible catalog ----------------------------------------
    // Three consumers answer "what should the model be told about?" and
    // every one of them starts from a registry read. These are the
    // numbers that say whether that read is a scan-and-clone or a memo.
    Bench {
        label: "ToolRegistry::snapshot (40 tools)",
        iters: 20_000,
    }
    .run("name+description clones + sort", || {
        black_box(black_box(&registry).snapshot());
    });

    // Warm the memo, then measure the accessor the catalog consumers use.
    let _ = registry.snapshot_cached();
    Bench {
        label: "ToolRegistry::snapshot_cached (40 tools)",
        iters: 2_000_000,
    }
    .run("per request, warm", || {
        black_box(black_box(&registry).snapshot_cached());
    });

    let grouped = {
        let grouped =
            synthia_tool::GroupedRegistry::new(Arc::new(tool_registry(40)));
        // `tool_registry(n)` names its tools `echo` + `tool_1..tool_{n-1}`.
        let names: Vec<String> =
            (1..=20).map(|i| format!("tool_{i}")).collect();
        let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
        grouped.declare("core", &borrowed).unwrap();
        grouped.activate("core");
        grouped
    };
    Bench {
        label: "GroupedRegistry::visible_tool_names (40 tools)",
        iters: 200_000,
    }
    .run("active group + unclaimed", || {
        black_box(black_box(&grouped).visible_tool_names());
    });

    Bench {
        label: "  ...from the uncached snapshot",
        iters: 20_000,
    }
    .run("what the memo replaced", || {
        let names: Vec<String> = grouped
            .inner()
            .snapshot()
            .into_iter()
            .filter(|snap| grouped.is_visible(&snap.name))
            .map(|snap| snap.name)
            .collect();
        black_box(names);
    });

    let adaptive = synthia_tool::AdaptiveRegistry::new(
        Arc::new(tool_registry(40)),
        synthia_provider::ModelTier::Small,
    );
    Bench {
        label: "AdaptiveRegistry::visible_tool_names (40→5)",
        iters: 200_000,
    }
    .run("tier cap, cached catalog", || {
        black_box(black_box(&adaptive).visible_tool_names());
    });

    // The same answer computed from the uncached catalog — the shape
    // this consumer used before the memo existed ("rebuild all forty,
    // keep five"). In-run baseline, so the pair is comparable.
    Bench {
        label: "  ...from the uncached snapshot",
        iters: 20_000,
    }
    .run("what the memo replaced", || {
        let names: Vec<String> = adaptive
            .inner()
            .snapshot()
            .into_iter()
            .take(adaptive.limits().max_visible_tools)
            .map(|snap| snap.name)
            .collect();
        black_box(names);
    });

    // ---- context window ---------------------------------------------
    Bench {
        label: "TruncatingContextManager (200 msgs, over window)",
        iters: 2_000,
    }
    .run("fits check + evict", || {
        let mut state = AgentState::with_window(2_000);
        let input = Arc::new(messages.clone());
        black_box(futures::executor::block_on(
            TruncatingContextManager.prepare_arc(input, &mut state),
        ));
    });

    Bench {
        label: "TruncatingContextManager (200 msgs, no eviction)",
        iters: 20_000,
    }
    .run("fast path", || {
        let mut state = AgentState::with_window(usize::MAX / 2);
        let input = Arc::new(messages.clone());
        black_box(futures::executor::block_on(
            TruncatingContextManager.prepare_arc(input, &mut state),
        ));
    });

    // ---- one whole turn ---------------------------------------------
    // The number a user feels: a complete run — system prompt, one tool
    // round trip, final answer — with the provider scripted so the
    // measurement is the framework's own work (context prep, tool
    // dispatch, event stream, typed sink).
    let turn_agent = scripted_turn_agent();
    Bench {
        label: "ReActAgent turn (scripted, 1 tool call)",
        iters: 500,
    }
    .run("end to end", || {
        black_box(run_one_turn(&turn_agent));
    });

    // ---- text truncation --------------------------------------------
    // `cap_to_char_boundary` truncates in place, so the measured unit is
    // "own a fresh 200 KB buffer and cut it to 100 KB" — what a tool
    // result buffer actually does. The clone is part of the number and
    // is called out in the note.
    Bench {
        label: "cap_to_char_boundary (200 KB → 100 KB)",
        iters: 50_000,
    }
    .run("clone + utf-8 safe cut", || {
        let mut buffer = text_200k.clone();
        cap_to_char_boundary(&mut buffer, 100_000);
        black_box(buffer.len());
    });

    println!();
}

/// A history shaped like a real tool-using turn: system prompt, then
/// user/assistant pairs whose assistant turns carry tool calls and whose
/// tool turns carry the results.
fn tool_heavy_history(pairs: usize) -> Vec<Message> {
    let mut messages = vec![Message::system("You are Synthia. ".repeat(20))];
    for i in 0..pairs {
        messages.push(Message::user(format!(
            "Step {i}: read the file and summarise it. {}",
            "context ".repeat(10)
        )));
        messages.push(Message::new(
            Role::Assistant,
            Content::parts(vec![ContentPart::ToolUse(
                synthia_provider::ToolUse {
                    id: format!("call-{i}"),
                    name: "read".to_string(),
                    input: serde_json::json!({ "path": format!("/workspace/file_{i}.rs"), "offset": i, "limit": 200 }),
                },
            )]),
        ));
        messages.push(Message::new(
            Role::Tool,
            Content::Single(ContentPart::ToolResult(ToolResult {
                tool_use_id: format!("call-{i}"),
                tool_name: Some("read".to_string()),
                content: vec![ContentPart::Text(TextContent {
                    text: "line of file content ".repeat(20),
                    cache_control: None,
                })],
                structured_content: None,
                is_error: None,
                metadata: serde_json::Map::new(),
                truncated_by: None,
            })),
        ));
        messages.push(Message::assistant(format!(
            "Summarised step {i}. {}",
            "summary ".repeat(10)
        )));
    }
    messages
}

/// A tool catalog with the three exposure levels represented, as the
/// server's registry produces them.
fn tool_descriptors(count: usize) -> Vec<ToolDescriptor> {
    (0..count)
        .map(|i| ToolDescriptor {
            name: format!("tool_{i}"),
            description: format!(
                "Tool number {i} does a thing. {}",
                "detail ".repeat(8)
            ),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "limit": { "type": "integer" },
                },
                "required": ["path"],
            }),
            category: ToolCategory::Utility,
            is_hidden: false,
            exposure: if i % 10 == 3 {
                ToolExposure::Deferred
            } else if i % 10 == 7 {
                ToolExposure::Hidden
            } else {
                ToolExposure::Direct
            },
            annotations: None,
        })
        .collect()
}

/// A nested tool-argument payload of roughly 4 KB.
fn nested_tool_args(depth: usize) -> String {
    let mut out = String::from("{");
    for i in 0..depth {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "\"key_{i}\":{{\"nested\":{{\"value\":\"{}\",\"index\":{i}}}}}",
            "x".repeat(20)
        ));
    }
    out.push('}');
    out
}

// ---------------------------------------------------------------------
// One whole turn: a scripted provider that asks for one tool and then
// answers, a registry large enough to make the tool-surface projection
// real work, and a spawner that does not need a runtime (the loop
// detaches its run through `Spawner`).
// ---------------------------------------------------------------------

/// Runs detached tasks on a small pooled executor — closer to a real
/// deployment than a fresh OS thread per turn, and still not tokio.
struct ThreadSpawner {
    pool: futures::executor::ThreadPool,
}

impl ThreadSpawner {
    fn new() -> Self {
        Self {
            pool: futures::executor::ThreadPool::new().expect("thread pool"),
        }
    }
}

impl synthia_core::spawn::Spawner for ThreadSpawner {
    fn spawn(&self, task: synthia_core::spawn::BoxFuture<()>) {
        self.pool.spawn_ok(task);
    }

    fn spawn_blocking(&self, f: Box<dyn FnOnce() + Send + 'static>) {
        std::thread::spawn(f);
    }
}

/// Provider that asks for `echo` on the first call and answers on the
/// second — one tool round trip per turn.
struct ScriptedProvider {
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl synthia_provider::ModelProvider for ScriptedProvider {
    async fn initialize(
        &mut self,
        _config: synthia_provider::ProviderConfig,
    ) -> Result<(), synthia_core::Error> {
        Ok(())
    }

    fn name(&self) -> &str {
        "scripted-bench"
    }

    fn model_config(&self) -> synthia_provider::ModelConfig {
        synthia_provider::ModelConfig {
            name: "scripted-1".to_string(),
            provider: "scripted".to_string(),
            context_window: 8_192,
            max_output_tokens: 1_024,
            supports_tools: true,
            supports_streaming: false,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        _request: synthia_provider::CompletionRequest,
    ) -> Result<synthia_provider::CompletionResponse, synthia_core::Error> {
        let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let content = if call.is_multiple_of(2) {
            Content::parts(vec![ContentPart::ToolUse(
                synthia_provider::ToolUse {
                    id: format!("call-{call}"),
                    name: "echo".to_string(),
                    input: serde_json::json!({ "text": "bench" }),
                },
            )])
        } else {
            Content::text("done")
        };
        Ok(synthia_provider::CompletionResponse {
            content,
            ..synthia_provider::CompletionResponse::default()
        })
    }
}

/// A registry with `echo` plus a realistic number of other tools, so
/// projection and `descriptors()` do representative work.
fn tool_registry(count: usize) -> synthia_tool::ToolRegistry {
    let registry = synthia_tool::ToolRegistry::new()
        .with_spawner(Arc::new(ThreadSpawner::new())
            as Arc<dyn synthia_core::spawn::Spawner>);
    for i in 0..count {
        let name = if i == 0 {
            "echo".to_string()
        } else {
            format!("tool_{i}")
        };
        registry.register_entry(synthia_tool::ToolEntry::new(Arc::new(
            synthia_test_support::FakeTool::new(&name, "ok"),
        )));
    }
    registry
}

fn scripted_turn_agent() -> synthia_harness::ReActAgent {
    let provider: Arc<dyn synthia_provider::ModelProvider> =
        Arc::new(ScriptedProvider {
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
    synthia_harness::ReActAgent::new(provider, Arc::new(tool_registry(40)))
        .with_workspace(".")
        .with_spawner(Arc::new(ThreadSpawner::new())
            as Arc<dyn synthia_core::spawn::Spawner>)
        .with_max_iterations(4)
        .with_name("bench")
}

/// Drive one turn to completion and return the number of events seen.
fn run_one_turn(agent: &synthia_harness::ReActAgent) -> usize {
    use synthia_core::{AtomicCancelToken, CancelToken};
    use synthia_harness::{Agent as _, AgentInput};

    let cancel: Arc<dyn CancelToken> = AtomicCancelToken::shared();
    let mut stream = futures::executor::block_on(
        agent.run(AgentInput::text("echo something, then answer"), cancel),
    );
    let mut events = 0usize;
    futures::executor::block_on(async {
        while futures::StreamExt::next(&mut stream).await.is_some() {
            events += 1;
        }
    });
    events
}
