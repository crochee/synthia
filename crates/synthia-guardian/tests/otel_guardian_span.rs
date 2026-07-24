#![cfg(feature = "otel")]
//! Integration tests for the `guardian.check` span emitted by
//! [`synthia_guardian::review::reviewer::GuardianReviewer::check`]
//! when the `otel` cargo feature is enabled.
//!
//! Verifies the span records the OTel Guardian semantic-convention
//! attributes required by the `agent-runtime-spans` spec:
//!  - `guardian.layer = "reviewer"` (set at span creation — known
//!    at compile time)
//!  - `guardian.decision` (`allow` / `deny` / `need_user_confirm`)
//!    recorded AFTER the inner check returns (declared as
//!    `tracing::field::Empty` at the callsite per the Task 7
//!    lesson — `Span::record` on an undeclared field is a silent
//!    no-op).
//!
//! Three decision paths are exercised:
//!  1. `allow`      — disabled Guardian returns `Allow` immediately.
//!  2. `deny`       — enabled Guardian with a failing `ModelRouter`
//!     fails closed to `Deny`.
//!  3. `need_user_confirm` — enabled Guardian with a mock
//!     `ModelProvider` whose LLM response parses to a medium-risk
//!     `Assessment` (risk_score 50-79), producing
//!     `NeedUserConfirm`.
//!
//! The capture layer records both the initial fields (via
//! `on_new_span`) and the fields populated later via `Span::record`
//! (via `on_record`). Field values are stringified through the
//! `tracing::field::Visit::record_debug` trait — string-literal
//! fields are captured with Debug formatting (surrounding quotes),
//! while `%`-recorded Display values are captured without quotes.
//! All assertions use `contains(...)` for robustness.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use synthia_core::Error as CoreError;
use synthia_guardian::{
    ApprovalRequest,
    GuardianConfig,
    GuardianDecision,
    review::reviewer::GuardianReviewer,
};
use synthia_model_router::{
    ModelConfig,
    ModelRouter,
    RoutingResult,
    types::{
        RoutingDecision,
        core::ProviderType,
        model::{ModelCapabilities, ModelInfo},
    },
};
use synthia_provider::{
    CompletionRequest,
    CompletionResponse,
    Content,
    ContentPart,
    Message,
    ModelProvider,
    ProviderConfig,
    TextContent,
    TokenUsage,
    types::ModelConfig as ProviderModelConfig,
};
use tracing_subscriber::{
    layer::{Context, Layer},
    prelude::*,
};

// =====================================================================
// Capture layer plumbing (mirrors the pattern in
// `synthia-provider/tests/otel_llm_span.rs` and
// `synthia-context/tests/otel_compaction_span.rs`).
// =====================================================================

#[derive(Default, Debug)]
struct CapturedSpan {
    name: String,
    fields: HashMap<String, String>,
}

struct CaptureLayer {
    spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>>,
}

struct FieldVisitor<'a>(&'a mut HashMap<String, String>);

impl tracing::field::Visit for FieldVisitor<'_> {
    fn record_debug(
        &mut self,
        field: &tracing::field::Field,
        value: &dyn std::fmt::Debug,
    ) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }
}

impl<S: tracing::Subscriber> Layer<S> for CaptureLayer {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        _ctx: Context<'_, S>,
    ) {
        let mut span = CapturedSpan {
            name: attrs.metadata().name().to_string(),
            fields: HashMap::new(),
        };
        let mut visitor = FieldVisitor(&mut span.fields);
        attrs.record(&mut visitor);
        self.spans.lock().unwrap().insert(id.clone(), span);
    }

    fn on_record(
        &self,
        span: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        _ctx: Context<'_, S>,
    ) {
        let mut spans = self.spans.lock().unwrap();
        if let Some(span_data) = spans.get_mut(span) {
            let mut visitor = FieldVisitor(&mut span_data.fields);
            values.record(&mut visitor);
        }
    }
}

/// Install a `CaptureLayer` as the thread-local default subscriber
/// and return the shared `spans` map for assertions. The returned
/// guard keeps the subscriber alive until dropped (must outlive the
/// `check()` call under test).
fn install_capture_layer() -> (
    Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>>,
    tracing::subscriber::DefaultGuard,
) {
    let spans: Arc<Mutex<HashMap<tracing::span::Id, CapturedSpan>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let guard = tracing_subscriber::registry::Registry::default()
        .with(CaptureLayer {
            spans: spans.clone(),
        })
        .set_default();
    (spans, guard)
}

/// Extract the single captured span named `guardian.check` from the
/// map. Panics if zero or more than one `guardian.check` span was
/// captured.
fn expect_guardian_check_span(
    spans: &HashMap<tracing::span::Id, CapturedSpan>,
) -> &CapturedSpan {
    let guardian_spans: Vec<_> = spans
        .values()
        .filter(|s| s.name == "guardian.check")
        .collect();
    assert_eq!(
        guardian_spans.len(),
        1,
        "expected exactly one guardian.check span, got {}: {:?}",
        guardian_spans.len(),
        guardian_spans.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    guardian_spans[0]
}

fn assert_field_contains(
    fields: &HashMap<String, String>,
    name: &str,
    needle: &str,
) {
    match fields.get(name) {
        Some(v) => assert!(
            v.contains(needle),
            "field `{name}` should contain {needle:?}, got {v:?}"
        ),
        None => panic!(
            "field `{name}` should be present, but was not recorded. \
             Captured fields: {fields:?}"
        ),
    }
}

// =====================================================================
// Mock router / provider helpers.
// =====================================================================

/// Router whose `route` always returns `Err`. Used to drive the
/// `check()` "fail closed" path that produces `GuardianDecision::Deny`.
struct FailingRouter;

#[async_trait]
impl ModelRouter for FailingRouter {
    async fn route(
        &self,
        _messages: &[Message],
    ) -> anyhow::Result<RoutingResult> {
        Err(anyhow::anyhow!("intentional router failure"))
    }

    fn available_models(&self) -> &[ModelConfig] {
        &[]
    }

    fn context_window(&self) -> usize {
        128_000
    }
}

/// Provider whose `complete` returns a `CompletionResponse`
/// carrying the supplied text body. Used to feed a pre-baked
/// `Assessment` JSON to `parse_assessment_response` inside
/// `GuardianReviewer::call_llm_internal`.
struct StubProvider {
    response_text: String,
}

#[async_trait]
impl ModelProvider for StubProvider {
    async fn initialize(
        &mut self,
        _config: ProviderConfig,
    ) -> Result<(), CoreError> {
        Ok(())
    }

    fn name(&self) -> &str {
        "stub-provider"
    }

    fn model_config(&self) -> ProviderModelConfig {
        ProviderModelConfig {
            name: "stub-model".to_string(),
            provider: "stub".to_string(),
            context_window: 128_000,
            max_output_tokens: 4_096,
            supports_tools: false,
            supports_streaming: false,
            supports_reasoning: false,
        }
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> Result<CompletionResponse, CoreError> {
        Ok(CompletionResponse {
            id: "stub-response".to_string(),
            model: "stub-model".to_string(),
            content: Content::Single(ContentPart::Text(TextContent {
                text: self.response_text.clone(),
                cache_control: None,
            })),
            usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                cached_prompt_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
            },
            cached: false,
        })
    }

    async fn embed(
        &self,
        _texts: Vec<String>,
    ) -> Result<Vec<Vec<f64>>, CoreError> {
        Err(CoreError::Internal(
            "StubProvider does not support embed".to_string(),
        ))
    }
}

/// Router whose `route` returns a `RoutingResult` containing the
/// supplied stub provider. Used to drive `check()` along the
/// successful LLM-review path.
struct StubRouter {
    provider: Arc<dyn ModelProvider>,
}

#[async_trait]
impl ModelRouter for StubRouter {
    async fn route(
        &self,
        _messages: &[Message],
    ) -> anyhow::Result<RoutingResult> {
        Ok(RoutingResult {
            provider: Arc::clone(&self.provider),
            config: ModelConfig::Custom {
                provider_type: "stub".to_string(),
                info: ModelInfo {
                    name: "stub-model".to_string(),
                    api_key: None,
                    base_url: None,
                    context_window: Some(128_000),
                    description: None,
                    capabilities: Some(ModelCapabilities { vision: None }),
                    temperature: Some(0.0),
                    max_tokens: 1024,
                },
            },
            decision: RoutingDecision {
                selected_model: "stub-model".to_string(),
                provider_type: ProviderType::Custom,
                reasoning: String::new(),
                matched_rules: Vec::new(),
                conversation_metrics: Default::default(),
            },
        })
    }

    fn available_models(&self) -> &[ModelConfig] {
        &[]
    }

    fn context_window(&self) -> usize {
        128_000
    }
}

// =====================================================================
// Tests.
// =====================================================================

/// Disabled Guardian returns `Allow` immediately (no LLM call).
/// Span MUST still be created with `guardian.decision = "allow"`
/// and `guardian.layer = "reviewer"`.
#[tokio::test]
async fn test_disabled_guardian_span_has_allow_decision() {
    let config = GuardianConfig::default().enabled(false);
    let reviewer = GuardianReviewer::new(config);

    let request =
        ApprovalRequest::shell("id", vec!["ls".to_string()], "/tmp", None);
    let router: Arc<dyn ModelRouter> = Arc::new(FailingRouter);

    let (spans, _guard) = install_capture_layer();
    let decision = reviewer.check(&request, &[], &router).await;

    assert!(
        matches!(decision, GuardianDecision::Allow),
        "disabled guardian should return Allow, got {decision:?}"
    );

    let spans = spans.lock().unwrap();
    let span = expect_guardian_check_span(&spans);
    let fields = &span.fields;

    assert_field_contains(fields, "guardian.layer", "reviewer");
    assert_field_contains(fields, "guardian.decision", "allow");
}

/// Enabled Guardian + failing router fails closed to `Deny`. Span
/// MUST record `guardian.decision = "deny"` and `guardian.layer =
/// "reviewer"`.
#[tokio::test]
async fn test_router_failure_span_has_deny_decision() {
    let config = GuardianConfig::default()
        .enabled(true)
        .with_risk_threshold(80);
    let reviewer = GuardianReviewer::new(config);

    let request =
        ApprovalRequest::shell("id", vec!["echo".to_string()], "/tmp", None);
    let router: Arc<dyn ModelRouter> = Arc::new(FailingRouter);

    let (spans, _guard) = install_capture_layer();
    let decision = reviewer.check(&request, &[], &router).await;

    match &decision {
        GuardianDecision::Deny { reason } => {
            assert!(
                reason.contains("LLM review error"),
                "expected LLM-review-error denial, got: {reason}"
            );
        }
        other => {
            panic!("expected Deny on router failure, got: {other:?}")
        }
    }

    let spans = spans.lock().unwrap();
    let span = expect_guardian_check_span(&spans);
    let fields = &span.fields;

    assert_field_contains(fields, "guardian.layer", "reviewer");
    assert_field_contains(fields, "guardian.decision", "deny");
}

/// Enabled Guardian + stub provider returning a medium-risk
/// `Assessment` (risk_score 65) produces `NeedUserConfirm`. Span
/// MUST record `guardian.decision = "need_user_confirm"` and
/// `guardian.layer = "reviewer"`.
#[tokio::test]
async fn test_medium_risk_span_has_need_user_confirm_decision() {
    // Assessment JSON parsed by `parse_assessment_response`. risk_score
    // 65 falls in the 50-79 medium-risk band -> `make_guardian_decision`
    // returns `NeedUserConfirm`.
    let assessment_json = serde_json::json!({
        "risk_level": "medium",
        "risk_score": 65,
        "rationale": "Moderate risk operation",
        "evidence": [],
    })
    .to_string();

    let provider: Arc<dyn ModelProvider> = Arc::new(StubProvider {
        response_text: assessment_json,
    });
    let router: Arc<dyn ModelRouter> = Arc::new(StubRouter { provider });

    let config = GuardianConfig::default()
        .enabled(true)
        .with_risk_threshold(80);
    let reviewer = GuardianReviewer::new(config);

    let request =
        ApprovalRequest::shell("id", vec!["echo".to_string()], "/tmp", None);

    let (spans, _guard) = install_capture_layer();
    let decision = reviewer.check(&request, &[], &router).await;

    assert!(
        matches!(decision, GuardianDecision::NeedUserConfirm { .. }),
        "medium-risk assessment should produce NeedUserConfirm, got {decision:?}"
    );

    let spans = spans.lock().unwrap();
    let span = expect_guardian_check_span(&spans);
    let fields = &span.fields;

    assert_field_contains(fields, "guardian.layer", "reviewer");
    assert_field_contains(fields, "guardian.decision", "need_user_confirm");
}
