## Why

当前 Synthia 项目在完成大规模重构后遗留了多个需要清理的问题：2 个 clippy lint 错误阻塞 CI、`synthia-tool/registry.rs` 1193 行代码质量低下、以及潜在的 performance 瓶颈。立即处理这些问题可以提升开发效率、避免 technical debt 积累，并为后续功能开发打下坚实基础。

## What Changes

**Clippy Error Fixes**
- From: `cargo clippy --workspace` 有 2 个 lint 错误导致 CI 失败
- To: lint 错误全部修复，CI 通过
- Reason: 阻塞开发流程，必须优先解决
- Impact: non-breaking，所有开发者受益

**Code Quality: Registry Split**
- From: `synthia-tool/registry.rs` 1193 行，单文件承担过多职责
- To: 拆分为多个内聚模块，保留 public API 不变
- Reason: 改善可维护性，降低修改风险
- Impact: non-breaking，内部重构不影响外部调用

**Architecture Review**
- From: permission system 刚完成重构，未有结构验证
- To: 提供架构 audit 报告，确认设计合理性
- Reason: 验证 recent refactor 的完整性
- Impact: non-breaking，输出为文档

**Performance Analysis**
- From: 无正式的 performance profiling
- To: 输出优化 proposal，明确瓶颈优先级
- Reason: 为后续优化提供方向指引
- Impact: non-breaking，分析阶段不修改代码

## Capabilities

### New Capabilities

- `clippy-cleanup`: 修复 `synthia-agent/src/agent_tools.rs` 中的 2 个 lint 错误（`unwrap_or_default`, `bind_instead_of_map`）
- `registry-refactor`: 将 `synthia-tool/registry.rs` 1193 行拆分为内聚的子模块
- `architecture-audit`: 审查 permission system 重构后结构、multi-agent 残留引用、task/scheduler 职责边界
- `performance-analysis`: Profile build time、memory cold storage、embedding 计算，输出优化提案

### Modified Capabilities

- （无 public API 变更）

## Impact

- **代码**：主要影响 `synthia-agent/src/agent_tools.rs`、`synthia-tool/src/registry.rs`
- **CI**：修复后 `cargo clippy` 应通过
- **架构**：`synthia-permission`、`synthia-task`、`synthia-agent` 的结构需 review
- **性能**：需引入 profiling 工具或依赖现有 `synthia-telemetry`