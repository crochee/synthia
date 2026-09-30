## Context

Synthia 是一个本地优先的 AI agent CLI。其 `synthia-agent` 通过 `StreamBuilder` 主循环驱动 ReAct 流程，工具输出（如 `bash`、`grep`、`read` 大文件）经 `synthia_context::truncate::truncate_output` 截断后直接进入 LLM 上下文。

勘探发现：`synthia-context` 已经具备基础的输出旁路能力：
- `truncate_output` 在内容超过 `TruncateConfig::max_bytes` 时会保留 head/tail，并把完整内容 spill 到 `cfg.temp_dir`。
- `spill_to_disk` 用 ULID 生成临时文件名。
- 摘要标记已经包含字节数、行数和文件路径。

当前不足：
1. 仅按字节阈值判断，没有行数阈值；对高密度短行输出不敏感。
2. 默认 `temp_dir` 是系统临时目录 `/tmp/synthia-truncate`，不是持久化的 `~/.synthia/tool-output/`。
3. 没有 7 天 retention/cleanup。
4. 文件权限未显式设为 `0o600`。
5. spill 路径是随机 ULID，未与 `session_id` + `tool_call_id` 绑定，不利于审计和复现。

OpenCode 的 `ToolOutputStore` 已验证一种更优模式：当输出超过阈值时，将完整内容写入文件系统，上下文只保留摘要和路径。本 change 在现有 `truncate` 模块基础上补齐上述差距，同时保持现有会话存储抽象不变。

## Goals / Non-Goals

**Goals:**
- 当工具输出超过 `MAX_BYTES=50KB` 或 `MAX_LINES=2000` 时，将完整输出旁路到文件系统。
- 上下文中的 tool result 仅保留头部/尾部摘要 + 文件路径引用。
- 与现有 `truncate_output` 统一入口，避免两套截断逻辑并存。
- 为后续规划状态机减少上下文噪音。
- 文件保留 7 天后自动清理。

**Non-Goals:**
- 不改写会话存储后端（保持抽象）。
- 不引入新的 LLM provider 能力。
- 不直接实现沙箱（本 change 只确保旁路文件路径未来可被沙箱审计）。
- 不做实时/多模态输出处理。

## Decisions

### D1：旁路文件存储位置
- **选择**：`~/.synthia/tool-output/<session-id>/<tool-call-id>.txt`
- **理由**：
  - 本地 CLI 优先，用户 home 目录是自然的单用户存储位置。
  - 与会话存储解耦，不破坏现有抽象。
  - 与 `tool_call_id` 绑定，便于审计、复现和模型通过 `read` 工具再次读取。
  - 便于未来多租户场景下迁移到对象存储（路径可配置）。
- **已考虑 alternative**：
  - 项目本地 `.synthia/tool-output/`：被拒绝，因为工具输出可能跨多个项目调用，且可能污染用户代码库。
  - 直接放 `/tmp`：被拒绝，因为需要 7 天保留期和可预测清理。
  - 新建独立 `ToolOutputStore` crate：被拒绝，因为现有 `truncate` 模块已是统一入口，复用成本更低。

### D2：阈值参数
- **选择**：`MAX_BYTES=50KB`、`MAX_LINES=2000`、`RETENTION=7 days`
- **理由**：直接采用 OpenCode 已验证参数，减少本地调优成本。
- **已考虑 alternative**：
  - 按模型上下文比例动态计算：更复杂，且会引入不确定性；CLI 场景下固定阈值更可控。

### D3：摘要格式
- **选择**：复用现有 `truncate_output` 的 head/tail 机制（默认各 100 行，可配置），中间插入 `[... N bytes / M lines truncated; full output at <path> ...]` 标记。
- **理由**：现有实现已具备该能力，改动最小；head/tail 模式符合 LLM 对首尾注意力更强的特性。
- **已考虑 alternative**：
  - 改为 60%/40% 比例：现有 `head_lines`/`tail_lines` 以固定行数配置更直观，保持兼容。
  - 用 LLM 生成摘要：增加延迟和成本，不适合每个 tool result。

### D4：与 `truncate_output` 的关系
- **选择**：复用并增强现有 `synthia_context::truncate::truncate_output`：增加 `max_lines` 阈值、持久化目录、文件权限、可审计路径。
- **理由**：项目 memory 要求“Truncate operations must be unified through synthia_context::truncate::truncate_output”；现有模块已是统一入口，避免两套逻辑。
- **已考虑 alternative**：
  - 在 `synthia-tool-orchestrator` 中独立判断：会导致截断逻辑分散。
  - 新建 `ToolOutputStore` crate：重复现有能力，增加维护成本。

### D5：模型如何读取完整输出
- **选择**：不新增专门 tool；模型如需完整输出，可通过现有 `read` tool 读取旁路文件路径。
- **理由**：最小改动；`read` tool  already 能读文件，且摘要标记以 `full output at <path>` 形式暴露。
- **已考虑 alternative**：
  - 新增 `fetch_full_output` tool：增加工具表面积，收益有限。

### D6：清理策略
- **选择**：`synthia-agent` 在 `StreamBuilder` 启动时异步扫描 `~/.synthia/tool-output/` 并删除超过 7 天的文件；每次工具执行阶段完成 offload 后异步触发清理。
- **理由**：简单可靠，不需要 cron 或外部调度；cleanup 不阻塞 ReAct 主循环。
- **已考虑 alternative**：
  - 精确 TTL per file：更复杂，CLI 场景下按目录扫描足够。
  - 在 `synthia-context` 内部隐式清理：会把 Tokio runtime 依赖引入纯工具函数，不合适。

## Risks / Trade-offs

[Risk] 旁路文件包含敏感信息（如环境变量、代码内容） → Mitigation: 文件权限设为 `0o600`；未来沙箱 change 可进一步限制目录访问。

[Risk] 模型忽略旁路路径提示，反复要求相同工具 → Mitigation: 摘要中必须显式包含路径和大小；结合现有 loop detector 识别重复调用。

[Risk] 旁路路径在不同机器/会话间不一致 → Mitigation: 路径使用相对或用户 home 可解析格式；不依赖绝对路径进入持久化消息。

[Trade-off] 写入磁盘增加少量 I/O → 接受理由：换取显著降低 LLM token 成本和上下文稳定性，ROI 明确。

[Trade-off] 7 天保留期可能占用磁盘 → 接受理由：50KB × 数千次调用 ≈ 百 MB 级，现代开发机可接受；清理机制控制上限。

## Migration Plan

本 change 不涉及部署变更，纯代码/配置变更。

1. 在 `synthia-context::truncate` 模块内增强 `TruncateConfig` 与 `truncate_output`：
   - 新增 `max_lines` 阈值。
   - 新增可选 `session_id` / `tool_call_id` 用于生成确定性路径。
   - 默认 `temp_dir` 改为 `~/.synthia/tool-output/`。
   - `spill_to_disk` 设置文件权限 `0o600`。
2. 在 `synthia-context::truncate` 新增同步/异步 cleanup 工具函数，供调用方使用。
3. 修改 `synthia-agent::stream_builder::builder::tool_execution::execute_and_emit`：
   - 构造带 `session_id`/`tool_call_id` 的 `TruncateConfig`。
   - 工具执行阶段开始时触发 cleanup。
4. 添加单元测试与集成测试：
   - 小输出不走旁路。
   - 大输出按字节/行数阈值旁路并生成摘要。
   - 文件权限为 `0o600`。
   - 过期文件被清理。
5. 跑 `cargo fmt`、`cargo clippy`、`cargo test`。

Rollback：回退到上一版本即可恢复旧截断行为；旁路文件可保留或手动删除。

## Open Questions

1. `truncate_messages` 在上下文组装阶段也可能触发 offload；是否应为其使用 ULID 回退路径还是跳过 offload？
2. 是否需要把旁路事件写入 telemetry/metrics？
