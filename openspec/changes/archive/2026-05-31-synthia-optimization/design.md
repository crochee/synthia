## Context

Synthia 是一个模块化的 Rust AI Agent 框架，采用 22-crate workspace 结构。最近完成了大规模重构（task system、permission system、删除 multi-agent），但遗留了以下问题需要清理：

**当前状态：**
- 2 个 clippy lint 错误阻塞 CI（`synthia-agent/src/agent_tools.rs`）
- `synthia-tool/registry.rs` 1193 行，职责过重亟需拆分
- 性能存在瓶颈：memory cold storage (sqlx)、embedding 计算、build time
- 架构刚完成重构，需要 review 评估结构合理性

**干系人：**
- 开发团队：需要 clean CI、更好的代码质量
- 用户：依赖框架性能和稳定性

## Goals / Non-Goals

**Goals:**
- 消除 clippy lint 错误，解除 CI 阻塞
- 提升代码质量：拆分大文件、减少耦合
- 评估架构重构后的结构合理性
- 识别性能瓶颈并提出优化方向

**Non-Goals:**
- 不做大规模重写或架构变更（当前是优化阶段，不是重构阶段）
- 不引入新的破坏性变更
- 不修改已有的 public API 行为

## Decisions

### D1：采用并行流方案处理优化任务

- **选择**：三条独立 Stream 并行执行（质量清理、架构审查、性能优化）
- **理由**：
  - Clippy 是 blocker，需独立快速修复
  - 大文件拆分和性能优化可并行探索，互不依赖
  - 架构 review 结论可反馈指导其他 stream 的优先级调整
- **已考虑 alternatives**：
  - 串行方案（A）：按依赖顺序处理 → 效率低，反馈周期长
  - 一次性方案（C）：先全面 audit → 响应慢，无法快速解决 blocker

### D2：Stream 1 质量清理的范围

- **选择**：优先修复 clippy errors + 拆分 `registry.rs`
- **理由**：clippy 是 CI blocker；registry.rs 是明显的技术债
- **已考虑 alternatives**：
  - 只修 clippy → 未解决根本代码质量问题
  - 同时做全量 dead code 清理 → 范围过大，可能引入 regression

### D3：Stream 2 架构审查的焦点

- **选择**：聚焦在 permission system 重构后结构、multi-agent 残留引用、task/scheduler 职责边界
- **理由**：这三个领域是最近大重构的核心，需要验证完整性
- **已考虑 alternatives**：
  - 做全量架构 audit → 范围过大，产出不聚焦
  - 只做代码覆盖检查 → 忽略架构设计问题

### D4：Stream 3 性能优化的优先级

- **选择**：按 build time → memory cold storage → embedding 顺序分析
- **理由**：build time 影响开发效率最广；memory 和 embedding 需 profiling 后再定
- **已考虑 alternatives**：
  - 同时优化三项 → 资源分散，无法集中解决关键瓶颈
  - 只做 build time 优化 → 忽略更严重的 runtime 性能问题

## Risks / Trade-offs

[Risk] clippy fixes 可能引入 subtle behavioral change → Mitigation: 同时运行测试套件验证
[Risk] 大文件拆分可能影响现有的 tool registry 调用方 → Mitigation: 保持 public API 不变，只做内部模块化
[Risk] 性能优化方向可能需要更深入的 profiling → Mitigation: 先做 analysis，proposal 中明确是否需要额外工具

[Trade-off] 并行流虽然高效，但资源竞争可能导致某些任务延迟 → 接受理由：总体时间收益大于临时资源竞争损失

## Migration Plan

N/A — 本 change 不涉及部署变更，为纯代码质量优化和架构审查。

**验证条件：**
1. `cargo clippy --workspace -- -D warnings` 通过
2. `cargo test --workspace` 通过
3. 无新的 regression 引入

## Open Questions

1. **Q: registry.rs 拆分的粒度？** — 需先分析当前 1193 行文件的职责组合，建议按 ToolRegistry 的子模块拆分
2. **Q: embedding 优化是否需要引入新的依赖？** — 取决于 profiling 结果，当前使用 `synthia-skill` 的 embedding 模块
3. **Q: 是否需要为 performance 建立 baseline metrics？** — 建议在 optimization 开始前建立 build time 和 memory 使用 baseline