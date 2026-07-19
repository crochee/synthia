## Why

Synthia 在本地 CLI 运行时，大工具输出直接进入 LLM 上下文，快速耗尽 token 预算并破坏前缀稳定性。OpenCode 已验证将超大输出旁路到文件系统、上下文仅保留摘要的模式。本 change 引入该机制，在保持现有会话存储抽象的前提下，降低上下文压力并保护 KV Cache 命中率。

## What Changes

**Tool Output 处理行为**
- From: 工具输出经 `truncate_output` 截断后整段进入上下文。
- To: 输出超过 `50KB` 或 `2000 行`时，完整内容写入 `~/.synthia/tool-output/`，上下文只保留 head/tail 摘要 + 文件路径。
- Reason: 减少上下文噪音，保护前缀稳定性，降低 token 成本。
- Impact: 非破坏性；模型仍可通过 `read` tool 读取完整输出。

**清理行为**
- From: 无专门清理。
- To: 启动时与写入时异步清理超过 7 天的旁路文件。
- Reason: 控制磁盘占用。
- Impact: 无用户可见行为变化。

## Capabilities

### New Capabilities
- `tool-output-offloading`: 工具大输出旁路存储与摘要生成，包含阈值判断、文件写入、过期清理。

### Modified Capabilities
- 无现有 spec 的 REQUIREMENTS 需要变更；本 change 是纯新增行为。

## Impact

- `synthia-context::truncate::truncate_output`：统一入口，集成旁路判断。
- 新增 `synthia-context::tool_output_store` 模块负责文件 I/O 与清理。
- `synthia-agent::stream_builder::builder::run`：启动时触发清理任务。
- 新增/修改单元测试与集成测试。
- 无 API 变更，无会话存储迁移。
