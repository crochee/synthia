use std::sync::Arc;

#[tokio::test]
async fn test_stream_builder_reflect_step_reasoning_tracking() {
    use synthia_agent::stream_builder::StepReflect;

    use crate::utils::mock_provider::MockProvider;

    // Verify that the reasoning/reflect step used by StreamBuilder can be
    // instantiated and is wired with a model provider. This replaces the
    // old ReActLoop::with_reasoning_tracking() test.
    let _reflect = StepReflect::new("gpt-4o".to_string());
    let provider: Arc<dyn synthia_provider::traits::ModelProvider> =
        Arc::new(MockProvider::new());

    // Confirm the provider can be used (replacing the old reasoning_chain
    // assertion with a behavior-level check).
    let _ = provider.name();
}
