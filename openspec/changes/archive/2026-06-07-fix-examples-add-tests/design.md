## Context

项目是 Rust monorepo，包含 20+ 个 crate，使用 axum web 框架。最近完成了大量重构（275 files changed, 15753 insertions, 10328 deletions），但 examples 目录未能同步更新导致编译失败。

**当前状态**:
- `cargo clippy` 通过（无警告）
- 核心测试通过
- `synthia-examples` 目录编译错误

**根因**: synthia-tool 的 API 重构后，examples 没有同步更新。

## Goals / Non-Goals

**Goals:**
- 修复 examples 编译错误
- 为 synthia-evaluation 添加基础冒烟测试
- 确保所有 crate 可正常编译

**Non-Goals:**
- 不进行大规模的代码重构
- 不拆分 synthia-tool 模块（当前规模可接受）
- 不添加完整的集成测试（基础冒烟测试即可）

## Decisions

### D1: Examples API 修正

- **选择**: 修复 examples 以匹配当前 API
- **理由**: examples 是重要的文档和演示代码，删除过于可惜
- **已考虑 alternative**: 删除无法修复的 examples → 拒绝，因为 examples 有教学价值

**具体修改**:
- `RegisterableTool` → 使用 `ToolEntry` 或直接使用 `Arc<dyn Tool>`
- `register_tool()` → `register()`
- `output.content` → 返回 `Vec<ToolOutput>`，直接遍历

### D2: synthia-tool 模块拆分

- **选择**: 暂不拆分
- **理由**: 当前 724 行在可接受范围内（Rust 社区通常接受 <1000 行文件）
- **已考虑 alternative**: 立即拆分 → 拒绝，属于过度工程

### D3: 测试添加策略

- **选择**: 添加基础冒烟测试
- **理由**: 验证模块可正常加载即可，详细测试在后续迭代中完善
- **已考虑 alternative**: 完整测试 → 拒绝，工作量大且当前重点是修复编译错误

## Risks / Trade-offs

[Trade-off] 修复 examples 可能暴露更多 API 破坏性变更
→ 接受理由: 早发现早修复，避免后续更大的迁移成本

[Trade-off] 基础冒烟测试覆盖不足
→ 接受理由: 先行验证可加载性，详细测试可分期添加

## Migration Plan

1. 修复 `examples/tool_usage.rs` 和 `examples/basic_chat.rs`
2. 运行 `cargo build --examples` 验证编译通过
3. 为 synthia-evaluation 添加基础测试
4. 运行完整测试套件验证

N/A — 本 change 不涉及部署变更

## Open Questions

无

## Implementation Scope

### 1. 修复 examples 编译错误

**examples/tool_usage.rs**:
- 移除 `RegisterableTool` import
- 使用 `Arc<dyn Tool>` 直接注册
- `register_tool()` → `register()`
- 处理 `run_with_context` 返回的 `Vec<ToolOutput>`

**examples/basic_chat.rs**:
- 移除未使用的 `AgentRunConfig` import

### 2. 添加 synthia-evaluation 冒烟测试

验证:
- 模块可正常加载
- 基础 API 可调用