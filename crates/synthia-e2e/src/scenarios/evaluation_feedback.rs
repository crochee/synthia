use std::sync::Arc;

use async_trait::async_trait;
use synthia_evaluation::{
    EvaluationInput,
    EvaluationRegistry,
    EvaluationScore,
    Evaluator,
};

#[tokio::test]
async fn test_evaluation_feedback_loop() {
    // 创建一个简单的 evaluator
    struct SimpleEvaluator;

    #[async_trait]
    impl Evaluator for SimpleEvaluator {
        fn name(&self) -> &str {
            "simple"
        }

        async fn evaluate(
            &self,
            _input: &EvaluationInput,
        ) -> Result<EvaluationScore, synthia_evaluation::EvaluationError>
        {
            Ok(EvaluationScore {
                criterion: "quality".to_string(),
                score: 0.85,
                rationale: "Looks good".to_string(),
            })
        }
    }

    // 注册 evaluator
    let registry = EvaluationRegistry::new();
    registry.register("simple".to_string(), Arc::new(SimpleEvaluator));

    // 创建评估输入并进行评估
    let input = EvaluationInput {
        task: "test_task".to_string(),
        agent_output: serde_json::json!("test_output"),
    };

    // 获取并使用 evaluator
    let evaluator = registry.get("simple").unwrap();
    let score = evaluator.evaluate(&input).await.unwrap();

    // 验证评分结果
    assert_eq!(score.criterion, "quality");
    assert!((score.score - 0.85).abs() < 0.001);
}
