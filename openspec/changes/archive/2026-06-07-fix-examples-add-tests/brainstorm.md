<!--
Raw capture of superpowers:brainstorming output.

本檔原樣捕捉 brainstorming skill 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorming: synthenia 项目优化

## 背景

用户询问当前项目有哪些需要优化的地方。通过项目探索发现：

- 项目是 Rust monorepo，包含 20+ 个 crate
- 使用 axum web 框架
- 主要组件：agent, tool, skill, task, session, permission, memory, mcp 等
- 最近有大量重构（git diff 显示 275 files changed, 15753 insertions, 10328 deletions）

## 项目状态检查

### ✅ 通过
- `cargo clippy` 通过（无警告）
- 核心测试通过

### ❌ 问题
- `synthia-examples` 目录下有编译错误

## 发现的问题

### Q1: Examples 编译错误

**问题**: examples 目录下的代码引用了不存在的 API

**错误详情**:
```
error: could not find `tool_adapter` in `synthia_tool`
error[E0432]: unresolved import `synthia_tool::registry::RegisterableTool`
error[E0599]: no method named `register_tool` found for struct `ToolRegistry`
error[E0609]: no field `content` on type `Vec<ToolOutput>`
error[E0609]: no field `is_error` on type `Vec<ToolOutput>`
```

**涉及文件**:
- `examples/tool_usage.rs`
- `examples/basic_chat.rs`

**根因**: synthia-tool 的 API 在重构后发生了变化，但 examples 没有同步更新

### Q2: synthia-tool 模块大小

**问题**: `synthia-tool/src/registry.rs` 文件较大（724 行）

**分析**:
- 实际行数：724 行
- 文件职责：工具注册、权限检查、并发控制
- 是否需要拆分：当前可接受，除非有特定性能问题

### Q3: synthia-task 调度职责

**问题**: 检查 dispatcher 和 scheduler 是否有职责重叠

**结论**: ✅ 无问题
- `dispatcher.rs` (695 行) 和 `scheduler.rs` (311 行) **已被删除**
- 当前结构：`task_tools.rs` + `registry.rs`，职责清晰

### Q4: 测试覆盖

**问题**: 部分模块缺乏测试

**涉及模块**:
- `synthia-evaluation`: lib.rs 几乎为空，无测试
- `synthia-telemetry`: 有 `span_hierarchy_integration.rs` 测试

## 设计决议

### D1: Examples 编译错误修复方案

**选项**:
1. 修复 examples 以匹配当前 API（推荐）
2. 删除无法修复的 examples

**决议**: 选择方案 1
- `RegisterableTool` → 使用 `ToolEntry`
- `register_tool()` → `register()`
- `output.content` → 遍历 `Vec<ToolOutput>`

### D2: synthia-tool 拆分

**选项**:
1. 立即拆分（过度工程）
2. 暂时不拆分（推荐）
3. 仅在性能问题时拆分

**决议**: 选择方案 2
- 当前 724 行在可接受范围内
- 拆分收益不明显

### D3: 测试添加

**选项**:
1. 为 evaluation 和 telemetry 编写完整测试（工作量较大）
2. 添加基础冒烟测试（推荐）
3. 暂时不添加

**决议**: 选择方案 2
- 添加基础的冒烟测试验证模块可加载
- 详细的测试在后续迭代中添加

## 优化清单

1. **修复 examples 编译错误** (Task #1)
   - `tool_usage.rs` API 修正
   - `basic_chat.rs` 移除未使用的 import

2. **synthia-tool 文件拆分** (Task #2)
   - 结论：暂不需要拆分

3. **Task 调度职责** (Task #3)
   - 结论：无重叠问题

4. **测试覆盖** (Task #4)
   - synthia-evaluation: 添加基础测试
   - synthia-telemetry: 已有测试，评估是否充分