# Model Router 模块

模型路由模块，根据任务类型和对话内容动态选择合适的模型。

## 核心组件

| 组件 | 类型 | 功能描述 |
|------|------|----------|
| `ModelRouter` | Trait | 路由入口 trait |
| `RoutingStrategy` | Trait | 路由策略 trait |
| `FirstModelRouter` | Struct | 默认路由实现（选择第一个模型） |
| `DefaultModelRouter` | Struct | 基于策略的路由实现 |
| `SimpleStrategy` | Struct | 简单策略 |
| `RuleBasedStrategy` | Struct | 基于规则的路由策略 |
| `AdaptiveStrategy` | Struct | 自适应路由策略 |
| `ConversationAnalyzer` | Struct | 对话复杂度分析器 |
| `ProviderFactory` | Struct | 模型提供者工厂 |
| `ModelConfig` | Enum | 模型配置 |
| `RoutingResult` | Struct | 路由结果 |
| `RoutingDecision` | Struct | 路由决策详情 |

## 模块结构

```rust
pub mod analyzer;       // ConversationAnalyzer
pub mod config_router;  // FirstModelRouter
pub mod factory;        // ProviderFactory
pub mod router;         // DefaultModelRouter
pub mod strategy;       // 路由策略
pub mod types;          // 类型定义
```

## 路由流程

```
对话内容 → ModelRouter::route()
              ↓
      RoutingStrategy::route()
              ↓
      ┌───────┼───────┐
      ↓       ↓       ↓
   Simple  RuleBased Adaptive
   Strategy Strategy Strategy
              ↓
      ConversationAnalyzer::analyze()
              ↓
      ProviderFactory::create()
              ↓
         RoutingResult
```

## ModelConfig 变体

```rust
pub enum ModelConfig {
    Anthropic(ModelInfo),
    OpenAI(ModelInfo),
    OpenAICompatible { info: ModelInfo, base_url: String },
    Custom { provider_type: String, info: ModelInfo },
}
```

## RoutingTrigger

```rust
pub enum RoutingTrigger {
    Keywords { words: Vec<String>, match_type: KeywordMatch },
    Complexity { level: ComplexityLevel, comparison: Comparison },
    ConsecutiveTools { count: usize, comparison: Comparison },
    ConsecutiveFailures { count: usize },
    FirstTurn,
    MessageLength { min: Option<usize>, max: Option<usize> },
    ToolFailure,
}
```

## 使用示例

```rust
use synthia_agent::model_router::{
    FirstModelRouter, DefaultModelRouter, SimpleStrategy,
    RuleBasedStrategy, AdaptiveStrategy, RoutingRule,
    RoutingTrigger, ProviderType, ModelConfig,
};
use rmcp::model::SamplingMessage;

async fn example() {
    // 简单路由
    let router = FirstModelRouter::default();
    let msg = SamplingMessage::user_text("Hello");
    let result = router.route(std::slice::from_ref(&msg)).await.unwrap();
}
```

## ProviderFactory

```rust
use synthia_agent::model_router::{ProviderFactory, ModelConfig};

let factory = ProviderFactory::new();
let config = ModelConfig::anthropic("claude-3-opus");
let provider = factory.create(&config).unwrap();
```
