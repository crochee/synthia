# Guardian 模块

安全审查模块，提供操作风险评估和审批功能，防止危险操作执行。

## 核心组件

| 组件 | 文件 | 功能描述 |
|------|------|----------|
| `Guardian` | [review.rs](review.rs) | 审查 trait |
| `SimpleGuardian` | [review.rs](review.rs) | 简单实现（基于规则） |
| `AdvancedGuardian` | [review.rs](review.rs) | 高级实现（AI 模型） |
| `GuardianReviewer` | [review.rs](review.rs) | AI 审查执行器 |
| `GuardianConfig` | [config.rs](config.rs) | 审查配置 |
| `GuardianRiskLevel` | [config.rs](config.rs) | 风险等级 |
| `GuardianMode` | [config.rs](config.rs) | 运行模式 |
| `ApprovalRequest` | [approval_request.rs](approval_request.rs) | 审批请求 |
| `Assessment` | [types.rs](types.rs) | 风险评估结果 |
| `Evidence` | [types.rs](types.rs) | 风险证据 |
| `ReviewDecision` | [types.rs](types.rs) | 审查决策 |
| `TranscriptEntry` | [transcript.rs](transcript.rs) | 对话记录条目 |

## 审查流程

```
工具调用请求 → 风险评估 → 决策 → 执行/拒绝
```

## 审批请求类型

```rust
pub enum ApprovalRequest {
    Shell { id, command, cwd, justification },
    ExecCommand { id, command, cwd, justification, tty },
    ApplyPatch { id, cwd, files, change_count, patch },
    NetworkAccess { id, turn_id, target, host, protocol, port },
    McpToolCall { id, server, tool_name, arguments, ... },
}
```

## 风险等级

```rust
pub enum GuardianRiskLevel {
    Low,
    Medium,   // 默认
    High,
}
```

## 运行模式

```rust
pub enum GuardianMode {
    Simple,                              // 基于规则的风险评估
    Advanced { model_name: String },     // AI 模型评估
}
```

## 审查决策

```rust
pub enum ReviewDecision {
    Approved,
    Denied { reason: String },
}
```

## 使用示例

```rust
use synthia_agent::guardian::{
    Guardian,
    SimpleGuardian,
    GuardianConfig,
    GuardianMode,
};
use std::sync::Arc;

let guardian: Arc<dyn Guardian> = Arc::new(SimpleGuardian::new(
    GuardianConfig::default()
        .enabled(true)
        .with_risk_threshold(80)
));

let decision = guardian.review(&cancel_token, request).await?;
match decision {
    Some(ReviewDecision::Approved) => {},
    Some(ReviewDecision::Denied { reason }) => {},
    None => {}, // 审查被禁用
}
```

## 配置

```rust
pub struct GuardianConfig {
    pub enabled: bool,           // 是否启用
    pub risk_threshold: u8,      // 风险阈值（默认 80）
    pub max_retries: u32,        // 最大重试次数（默认 3）
    pub mode: GuardianMode,      // 运行模式
}
```

## 测试

```bash
cargo test -p synthia-agent guardian:: --lib
```
