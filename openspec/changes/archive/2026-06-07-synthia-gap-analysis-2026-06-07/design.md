# Design: Synthia Gap Analysis Implementation

> 输入：`brainstorm.md`（raw 决策日志）
> 输出：架构决策、迁移路径、未决问题
> 关联：`proposal.md`（动机/能力）、`specs/*.md`（具体行为）、`tasks.md`（执行顺序）

---

## Context

Synthia 是用 Rust workspace 实现的 AI Agent 框架，已完成 27 个 openspec change 的落地。OpenCode（TypeScript, 50+ crates）和 Codex（Rust, codex-rs 30+ crates）是两个生产级标杆。

**当前状况**（详见 `brainstorm.md`）：
- 骨架完整，5+ 套 prompt 构建、3 套 compaction、4 套 truncate、15 个 token 估算文件并存
- 3 个 Critical 硬 bug（`is_concurrency_safe` 硬编码 false、PrefixTracker 孤岛、`trim_to_budget` O(n²)）
- P1（prefix 稳定）和 P6（不信任 LLM）落不到地，因为同概念在 3-5 处各自实现

**用户选择**：战略 1（基础收敛）+ 工作粒度 = 先写完整设计文档。

**目标**：在 4 个能力上落地 P0 补齐 — 收敛 5 套 prompt、Tool trait 增并发声明、PrefixTracker 真正接入、Token 计数单一化。

---

## Goals / Non-Goals

### Goals

1. **G1**：5 套 prompt 构建路径收敛为 1 套 `ContextAssembler` 入口
2. **G2**：`Tool` trait 增 `is_concurrency_safe` 默认 `false`，4 个 builtin 显式声明
3. **G3**：`PrefixTracker` 真正接入 `StreamBuilder` LLM 调用生命周期
4. **G4**：15 个 token 估算文件收敛为 `synthia-provider::TokenCounter` 单 trait
5. **G5**：每个能力 ≥ 6 unit tests + ≥ 1 integration test
6. **G6**：公开 API 完全向后兼容
7. **G7**：每个能力独立 commit，可单独 revert

### Non-Goals

- 不修 `trim_to_budget` O(n²)（下一个 critical change）
- 不修 `pruning::hard_clear` 静默丢内容（属 P8 改造）
- 不合 3 套 compaction（语义差异需先确定）
- 不动 Guardian / Rollout / Plugin
- 不升级 Permission 粒度（已有 spec 部分处理）
- 不修 bash UTF-8 panic / read_history 无界（属"安全稳定" change）

---

## Decisions

### D1: 收敛模式 = "单入口 + 删除其他"

**决策**：5 套 prompt 构建 → 只留 `ContextAssembler`；agent 私有 `ContextBuilder` 删除。

**为什么单入口**：
- 5 套并存 = 5 个不同调用点，新 section 改 5 处
- P1 (prefix 稳定) 需要"前 N 个 section 字节级不变"，多入口 = 多破坏点
- OpenCode 也是单 `ContextAssembler` + Plugin 注入式扩展

**为什么不"统一 trait"**：
- 5 套的实现都把"组装"和"渲染"耦合在一起，强行抽象 trait 反而把简单的事变复杂
- 收敛是删除代码，不是增加抽象

**约束**：
- 迁移前先 e2e 锁定每条旧路径的行为
- 删前确保 `cargo test --workspace` 通过
- `ContextAssembler` 是唯一 `pub` 入口

### D2: `is_concurrency_safe` 放在 trait 方法

**决策**：在 `synthia-tool::traits::Tool` 加 `fn is_concurrency_safe(&self) -> bool { false }` 默认方法。

**为什么 trait 方法**：
- 运行时可反映状态（如 read 在 lockfile 存在时变 unsafe）
- 默认 `false` 保持向后兼容（旧实现自动无感）
- 4 个 builtin 显式 override

**为什么不 struct 字段**：
- ToolEntry 重复声明，状态不同步
- 编译期可知性对 agent 调度器无意义（dispatcher 本身就是动态的）

**builtin 标注**：
```rust
// synthia-tool/src/builtin/read.rs  → is_concurrency_safe: true
// synthia-tool/src/builtin/glob.rs  → true
// synthia-tool/src/builtin/grep.rs  → true
// synthia-tool/src/builtin/web.rs   → true
// synthia-tool/src/builtin/bash.rs  → false (always)
// synthia-tool/src/builtin/write.rs → false (always)
// synthia-tool/src/builtin/multi_edit.rs → false
// synthia-tool/src/builtin/path.rs      → true (read-only)
```

### D3: PrefixTracker 接入点 = `StreamBuilder::run` 内 LLM 调用边界

**决策**：`StreamBuilder::run` 在调用 `model_call` 前调 `prefix_tracker.record_pre(system_snapshot)`，调用后调 `record_post(system_snapshot)`，差异上报 telemetry。

**为什么 LLM 调用边界**：
- cache 命中发生在 LLM 提供方（Anthropic/OpenAI），前缀是 send 出去的字节
- 我们只能观测"我们发出去的 prefix"是否稳定
- 观测点必须是 send 前后的真实 prefix

**为什么不放在 `ContextAssembler`**：
- Assembler 是"组装"，但 prefix 稳定是"传输"属性
- 把传输层观测放传输层（LLM client wrapper）更直接
- Assembler 只保证"组装结果稳定"，但 provider transform 可能在中间改 prefix

**record 频率**：每次 LLM 调用（不轮 token，因为 token counter 不可靠；轮 prefix 字节级 hash）
**窗口**：`stability_ratio` rolling 20 turn（OQ2 答复）

### D4: TokenCounter trait 放 `synthia-provider`

**决策**：
```rust
// synthia-provider/src/token_counter.rs
pub trait TokenCounter: Send + Sync {
    fn count_messages(&self, msgs: &[Message]) -> u32;
    fn count_text(&self, text: &str) -> u32;
}
```

`AnthropicCounter` / `OpenAITokenCounter` 各自 impl；`synthia-context` 通过 `Arc<dyn TokenCounter>` 注入。

**为什么 provider**：
- BPE 知识天然在 provider（Anthropic 用自家 tokenizer，OpenAI 用 tiktoken）
- 改 provider 时 token 计数自然跟着切
- 避免在 context 层维护 3-4 套精确度差异巨大的估算

**为什么不 core**：
- core 已经是 type/error/config 的薄层，加 token 计数会胖
- provider 已经有消息转换（Message → provider wire format），counter 是同一类活

**约束**：
- `synthia-context → synthia-provider`：同 workspace，无循环
- `Arc<dyn TokenCounter>` 注入，不在 `ContextAssembler::new` 里 hardcode

### D5: 实施顺序

**为什么这个顺序**：
1. **C4 Token 单一化** 先做：它影响面最小（仅 trait 引入），且 C1/C2/C3 都需要准确的 token 计数
2. **C2 Tool trait** 第二：它是单方法扩，向后兼容，零风险
3. **C1 Prompt 收敛** 第三：删 4 套，5 个调用点逐个迁移，需 e2e 锁定
4. **C3 PrefixTracker 接入** 最后：依赖 C1 收敛后的稳定 prefix

**为什么 C3 最后**：
- prefix 观测的"真值"是 send 出去的字节
- 如果 C1 未收敛，5 套 assembler 各算各的 prefix，观测噪声大
- 收敛后再观测，命中率可信

---

## Risks / Trade-offs

### R1: `Tool` trait 扩方法 → 旧 `impl Tool` 编译失败？

**缓解**：默认 `fn is_concurrency_safe(&self) -> bool { false }` 是 default method，旧实现不需改动。✅ **已消除风险**

### R2: C1 收敛漏掉某条调用路径

**缓解**：
- tasks.md 阶段 3 列每条旧路径的 e2e 锁定测试
- 删除前 `cargo test --workspace` 必须绿
- 任何漏掉的调用点编译会失败（删除私有 API → 编译错）

### R3: `synthia-context → synthia-provider` 引入循环？

**分析**：
- `synthia-provider` 已有依赖：`tokio`, `reqwest`, `serde`, `async-trait`
- `synthia-context` 当前不依赖 provider
- `synthia-provider` 不依赖 context ✅ 无循环

**缓解**：用 `Arc<dyn TokenCounter>` 注入，不在 `ContextAssembler` struct 字段直接 hold provider 类型。

### R4: PrefixTracker 接入后发现 cache 命中率 < 50% 怎么办？

**应对**：**这是好事**——暴露问题才能优化。**不阻塞本 change**。本 change 的目标是"可观测"，优化命中率是后续 change 的事。

### R5: 4 个能力并发实施 → conflict？

**应对**：tasks.md 强制 C4 → C2 → C1 → C3 顺序。**不并发实施**。

### R6: `ContextAssembler` public 扩方法破坏 API？

**分析**：
- 加 `section_by_name(&self, name: &str) -> Option<&Section>` 是新方法
- 已有 `assemble(...)` / `with_section(...)` 不变
- ✅ 完全向后兼容

### R7: TokenCounter trait 抽象引入 1 次虚函数调用

**分析**：
- 每次 LLM 调用前 1 次，开销 < 1µs
- 相比 LLM 调用的 100ms+，可忽略
- ✅ 不阻塞

---

## Migration Plan

### 阶段 1：trait 引入（无破坏，约 1 天）

- D2：`Tool` 加 `is_concurrency_safe` 默认方法
- D4：`synthia-provider::TokenCounter` trait
- 全部 `impl Tool` 走默认 `false`，全部 `estimate_message_tokens` 仍 pub
- **无任何行为变化**

### 阶段 2：迁移 + 验证（约 2-3 天）

- D2：4 个 builtin 显式标注 + 单测
- D4：`synthia-context` 改用 trait，删除 `estimator` 重复
- `cargo test --workspace` 通过
- `cargo clippy --all-targets` 无 warning

### 阶段 3：收敛（约 3-4 天）

- D1：删 agent 私有 `ContextBuilder`，5 个调用点逐个迁移
- 每迁一个跑 `cargo test -p synthia-agent`
- D3：`PrefixTracker` wire 到 `StreamBuilder::run`
- 新增 `prefix_stability_ratio` 指标

### 阶段 4：e2e 验证（约 1 天）

- `cargo test --workspace --all-features`
- 跑所有现有 e2e tests
- 新增 4 个 integration test

### 回滚

- 每个能力独立 commit
- 公开 API 完全向后兼容
- 任何中间状态可单独 revert

---

## Architecture Diagram

### 当前（5 套 prompt + 2 套 prefix）

```
LLM Call
  ├── ContextAssembler (876 行, 全功能)  ──┐
  ├── context::prompt::builder            │
  ├── context::system_context             ├── 5 套并存
  ├── agent::ContextBuilder (33 行)       │
  └── agent::AgentBuilder (占位)         ──┘
                                          ↓
                              system_prompt 字节级 prefix
                                          ↓
                              PrefixTracker (孤岛，无人调)
                                          ↓
                              telemetry::context_trace (另写一套)
```

### 目标（1 套 prompt + prefix 真正接入）

```
LLM Call
  └── ContextAssembler (唯一入口)
       ├── section_by_name(name) → Option<&Section>  [新]
       ├── system_snapshot() → &[u8]                  [新]
       └── assemble(budget) → Messages
                                          ↓
                              send 前后调 PrefixTracker
                                          ↓
                              prefix_stability_ratio → telemetry
```

### Token 计数收敛

```
当前 15 文件：
  - synthia-provider/{openai, anthropic, token_counter}
  - synthia-core/src/token
  - synthia-context/{traits, estimator, compactor, injector, ...}
  - synthia-agent/compaction
  - synthia-model-router/analyzer

目标 1 trait：
  - synthia-provider::TokenCounter (trait)
  - AnthropicCounter / OpenAITokenCounter (impls)
  - synthia-context 通过 Arc<dyn TokenCounter> 注入
```

### Tool 并发安全

```
当前：
  Tool trait { requires_permission }
                       ↓
  agent/step.rs:194-200
    let _is_concurrency_safe = tool.requires_permission();
    tool_infos.push(... false /* 硬编码 */);
                       ↓
  全部 Serial，parallel 失效

目标：
  Tool trait { requires_permission, is_concurrency_safe: bool }
                       ↓
  agent/step.rs
    tool_infos.push(... tool.is_concurrency_safe());
                       ↓
  builtin 显式声明：
    read/glob/grep/web/path → true
    bash/write/multi_edit   → false
```

---

## Open Questions

- **OQ1**: `ContextAssembler` 是否需要新加 `section_by_name`？（stream_builder 自反思用）
  - **倾向**：加，spec 已包含
  - **理由**：自反思需要"拿到 system_prompt section 内容生成 sub-goal"

- **OQ2**: `prefix_stability_ratio` 窗口大小？
  - **倾向**：rolling 20 turn
  - **理由**：20 turn ≈ 5-10 分钟交互，足够反映"典型 session 稳定性"

- **OQ3**: `TokenCounter::count_messages` 是单条还是 batch？
  - **倾向**：batch（`count_messages(&[Message]) -> u32`）
  - **理由**：调用方循环更简单；batch 可分摊函数调用开销

---

## Verification

- 4 个能力各 ≥ 6 unit tests（行为锁定）
- 4 个 integration tests（端到端：prefix 稳定性、并发调度、token 计数、prompt 组装）
- 全部现有 e2e tests 通过
- `cargo clippy --all-targets --all-features --tests --all` 无 warning
- 公开 API diff：`git diff main -- crates/*/src/lib.rs` 只增不改
