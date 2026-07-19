## Why

synthia-examples 编译失败影响开发体验和文档价值。最近的项目重构更改了 synthia-tool 的 API，但 examples 未同步更新。同时 synthia-evaluation 模块缺乏测试，需要添加基础冒烟测试验证其可用性。

## What Changes

**修复 examples 编译错误**
- From: examples 引用不存在的 API（`RegisterableTool`, `register_tool`, `tool_adapter`）
- To: examples 使用当前正确的 API
- Reason: examples 是重要的文档和演示代码，编译失败影响开发体验
- Impact: 非破坏性变更，仅修复引用

**添加基础冒烟测试**
- From: synthia-evaluation 无测试
- To: 基础冒烟测试验证模块可加载
- Reason: 确保新模块在后续迭代中可以正常回归
- Impact: 非破坏性变更

## Capabilities

### New Capabilities
- `evaluation-smoke-test`: 为 synthia-evaluation 添加冒烟测试，验证模块可正常加载和基础 API 可调用

### Modified Capabilities
无

## Impact

**修复影响**:
- `examples/tool_usage.rs` - API 修正
- `examples/basic_chat.rs` - 移除未使用 import

**测试影响**:
- 新增 `synthia-evaluation` 冒烟测试

**无影响范围**:
- synthia-tool 模块结构（暂不拆分）
- synthia-task 调度逻辑（已验证无重叠）