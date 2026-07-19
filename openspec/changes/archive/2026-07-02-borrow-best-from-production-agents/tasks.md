## 1. 阶段 1：修复静默风险（前置必做）

- [x] 1.1 在 `crates/synthia-agent/src/agent.rs` 的 `Agent::run_stream` 中添加 orchestrator 缺失时自动调用 `assemble_default_tool_orchestrator` 的分支，并 `tracing::warn!` 日志 "auto-assembled tool orchestrator (caller did not inject one)"
- [x] 1.2 修改 `crates/synthia-agent/src/stream_builder/builder/run/main_loop.rs:191-194` 从手动恢复 2 字段改为 `LoopContext::from_metadata(metadata)` 完整恢复 4 字段（iteration / end_reason / cumulative_tokens / context_token_limit）
- [x] 1.3 新增 H1 回归测试：`run_stream` 未注入 orchestrator 时验证自动装配触发 + warning 日志产出 + 装配后 orchestrator 提供等效 sandbox/approval/retry
- [x] 1.4 新增 H4 回归测试：`metadata.iteration = 50` + `max_iterations = 50` 恢复后下个迭代立即触发 `MaxIterationsReached`，且 `cumulative_tokens` / `context_token_limit` 正确恢复
- [x] 1.5 运行 `cargo test -p synthia-agent` + `cargo clippy -p synthia-agent --all-features` 验证阶段 1 改动

## 2. 阶段 2：即时高 ROI 小改动

- [x] 2.1 在 `crates/synthia-provider` 的 `apply_cache_policy` 中添加 `Arc::ptr_eq` 引用相等短路：tools/system/messages 三者都 ptr_eq 时直接返回原引用，跳过完整 cache policy 评估
- [x] 2.2 在 `crates/synthia-tool-exec-base` 新增 `FileMutationQueue` 类型：`Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>`，使用 `std::fs::canonicalize` 解析 realpath 作为 key（处理 symlink）
- [x] 2.3 在 `ToolAdapter::execute` 层集成 `FileMutationQueue`：对 `write_file` / `apply_patch` / `edit_file` 工具强制 acquire per-filepath 锁后才执行
- [x] 2.4 实现 Map 清理逻辑：mutex 释放后若 `Arc::strong_count == 1`（无其他等待者）则从 Map 移除条目，防内存泄漏
- [x] 2.5 新增 Cache Policy 短路单测：三 Arc 都 ptr_eq 时验证返回原引用（用 `Arc::as_ptr` 比对）+ 任一 Arc 变更时验证走完整评估路径
- [x] 2.6 新增 file mutation queue 并发测试：同 filepath 串行化（验证第二个调用 block）+ 不同 filepath 并行（验证无 block）+ symlink 共享 realpath key
- [x] 2.7 新增 Map 清理测试：单次写完成后验证条目移除 + 有等待者时验证条目保留
- [x] 2.8 运行 `cargo test -p synthia-provider -p synthia-tool-exec-base` + `cargo clippy` 验证阶段 2 改动

## 3. 阶段 3：中期补强

### 3.1 Permission "always" 传播

- [x] 3.1.1 在 `crates/synthia-permission` 实现 "always allow" 后扫描同 session pending：新规则下 resources 全部 allow 时自动 resolve（不弹窗）
- [x] 3.1.2 实现 "reject" / "always reject" 级联终止：同 session 所有 pending 标记 `cascade-from-session-reject` + 对应 tool 调用返回 permission-denied
- [x] 3.1.3 新增测试：identical resources 自动 resolve + overlapping resources 不 resolve + cross-session 隔离 + reject 级联 + cross-session 不级联

### 3.2 Anchored Summary 8 段式

- [x] 3.2.1 在 `crates/synthia-context` 实现 8 段式模板：Goal / Constraints / Progress(Done/InProgress/Blocked) / Key Decisions / Next Steps / Critical Context / Relevant Files
- [x] 3.2.2 实现 LLM prompt 模板：无 `previousSummary` 时用 "Generate the anchored summary" + 8 段结构强制
- [x] 3.2.3 实现增量更新 prompt：有 `previousSummary` 时用 "Update the anchored summary" + 保留未变更段
- [x] 3.2.4 实现 token-budget aware split：优先 message 边界切分，单 message 超限时 mid-message slicing + `[truncated-mid-message]` marker + UTF-8 边界保护
- [x] 3.2.5 新增测试：8 段结构完整性 + 空段 placeholder + 增量更新保留未变段 + message 边界切分 + mid-message UTF-8 安全

### 3.3 Context overflow 检测

- [x] 3.3.1 在 `crates/synthia-provider` 新增 `ContextOverflowDetector`：21 provider-specific 正则（Anthropic/OpenAI/Google/其他）+ 集中管理 + 注释标注来源
- [x] 3.3.2 实现 3 排除模式：throttling / rate limit / "too many requests" 即使含 "context" 或 "length" 也不分类为 overflow
- [x] 3.3.3 实现 silent overflow 检测：`usage.input_tokens + usage.cache_read_tokens > context_window_tokens` 时触发截断
- [x] 3.3.4 实现 orphan tool call 合成空 result：truncation 移除 tool_result 时合成 `"[orphan tool call - result truncated]"` + 正确 `tool_use_id`
- [x] 3.3.5 新增测试：21 正则匹配（每 provider 至少 1 case）+ 3 排除 + silent overflow + orphan 合成 + 下次 API 调用无 "missing tool_result" 错误
- [x] 3.3.6 运行 `cargo test -p synthia-permission -p synthia-context -p synthia-provider` + `cargo clippy` 验证阶段 3 改动

## 4. 阶段 4：长期架构补强

### 4.1 TurnTransition defect 通道

- [x] 4.1.1 在合适 crate（synthia-agent 或新 crate）定义 `TurnTransition` 枚举：`ContextOverflow` / `ToolExecutionFailure(err)` / `FatalError(err)` 等变体
- [x] 4.1.2 定义 turn 返回类型为 `Result<TurnOutput, ControlFlow<TurnTransition>>`：Continue = 可恢复 defect，Break = 不可恢复
- [x] 4.1.3 实现外层 catchDefect 等价 match：Continue 触发 compaction + 重试，Break 终止 + 传播 error
- [x] 4.1.4 实现重试上限 3 次：不同 defect 类型共享计数，第 4 次转为 Break + 错误 "max defect retries (3) exceeded"
- [x] 4.1.5 新增测试：Continue 重试 + Break 终止 + 第 4 次拒重试 + 跨类型共享计数 + 第 3 次成功重置计数

### 4.2 CompactionAnalyticsAttempt 遥测

- [x] 4.2.1 在 `crates/synthia-telemetry` 定义 `CompactionAnalyticsAttempt` struct：5 字段（active_context_tokens_before / trigger / reason / implementation / phase）
- [x] 4.2.2 在 compaction 各阶段 emit 记录：Stage 1 soft-trim / Stage 2 hard-clear / Stage 3 pruning / anchored-summary，每阶段独立 `CompactionAnalyticsAttempt`
- [x] 4.2.3 实现 OTel emission（otel feature 启用时）：5 属性 `compaction.active_context_tokens_before` / `compaction.trigger` / `compaction.reason` / `compaction.implementation` / `compaction.phase` 注入 span
- [x] 4.2.4 实现 info! 日志 fallback（otel 禁用时）：5 字段统一格式日志
- [x] 4.2.5 新增测试：5 字段记录 + OTel emission + info! fallback + stage 升级各阶段独立记录 + trigger 区分 auto-threshold / tool-call / manual

### 4.3 SpanAttributesProcessor

- [x] 4.3.1 在 `crates/synthia-telemetry`（otel feature 内）实现 `SpanAttributesProcessor`：实现 `tracing_subscriber::Layer` 的 `on_new_span`（等价 on_start）
- [x] 4.3.2 在 on_start 注入 6 属性：`session.id` / `user.id` / `agent.id` / `turn.id` / `gen_ai.system` / `gen_ai.request.model`
- [x] 4.3.3 处理 missing context：`user.id` 等不可用时设为空字符串 `""`，不 panic
- [x] 4.3.4 剥离 Statsig 分支：grep 确认无 `statsig` 符号 + `Cargo.toml` 无 statsig 依赖
- [x] 4.3.5 验证 OTLP exporter 兼容性：`grpc://` scheme → gRPC exporter（4317 端口）+ `http://` scheme → HTTP exporter + 无 scheme 默认 gRPC
- [x] 4.3.6 新增测试：on_start 6 属性 + missing context 空字符串 + 无 statsig 依赖 + gRPC/HTTP 切换

### 4.4 SystemContext typed source

- [x] 4.4.1 在 `crates/synthia-context` 定义 `Source` trait：5 函数 `key(&self) -> &str` / `load(&self) -> Result<Value>` / `baseline(&self) -> Value` / `update(&self, prev: &Value) -> Result<Option<Value>>` / `removed(&self) -> bool`，约束 Value: `PartialEq + serde::Serialize + serde::Deserialize`
- [x] 4.4.2 定义 `Snapshot` struct：`encoded_value: Value` + `revision: u64` + serde 序列化
- [x] 4.4.3 定义 `SystemContext` 注册器：`HashMap<String, Box<dyn Source>>` + `HashMap<String, Snapshot>` 持久化
- [x] 4.4.4 实现 `reconcile` 函数：返回 `Unchanged` / `Updated` / `ReplacementReady(Snapshot)` / `ReplacementBlocked` 4 状态，使用 `PartialEq` 比较
- [x] 4.4.5 实现 `EnvironmentSource` 作为首个示例：`key="environment"` + `load()` 读环境变量 + `baseline()` 初始快照 + `update()` diff 检测
- [x] 4.4.6 集成到 system prompt 构建路径：reconcile 返回 Updated 时触发 system prompt 重新生成
- [x] 4.4.7 确认 SystemContext 不注册为 tool：tool registry 查询无 `update_system_context` 或类似名称 + Source 更新由 runtime event 触发非 LLM 调用
- [x] 4.4.8 新增测试：Source trait 5 函数 + Snapshot 序列化/反序列化 + reconcile 4 状态 + EnvironmentSource 端到端 + 不注册为 tool + ReplacementBlocked 警告日志
- [x] 4.4.9 运行 `cargo test --workspace --all-features` + `cargo clippy --all-features` 验证阶段 4 改动

## 5. 阶段 5：Tool 化改造

### 5.1 Guardian as Tool

- [x] 5.1.1 在 `crates/synthia-guardian` 暴露 `self_reflect` tool 定义：tool name `self_reflect` + 描述含 "independent context review" + schema 无 required 参数
- [x] 5.1.2 在 `crates/synthia-agent` tool 注册器注册 `self_reflect` + 实现 `Tool::call` dispatch 到 Guardian 评审逻辑
- [x] 5.1.3 保留每 5 轮 auto-trigger 兜底：`iter % 5 == 0` 且 LLM 未调用时注入 synthetic `tool_use` + 走相同 dispatch 路径
- [x] 5.1.4 实现 counter 重置：LLM 调用 `self_reflect` 后下次 auto-trigger 安排在 `current_iter + 5`
- [x] 5.1.5 同 iter 去重：LLM 已调用时跳过 auto-trigger，避免重复 review
- [x] 5.1.6 新增测试：tool 注册 + LLM 调用 dispatch + auto-trigger iter 5 + counter 重置 + 同 iter 去重 + LLM 调用与 auto-trigger 调用路径一致

### 5.2 Compaction as Tool

- [x] 5.2.1 在 `crates/synthia-context` 暴露 `compact_context` tool 定义：tool name `compact_context` + 可选 `reason: String` 参数 + 描述含 `<context_tokens>X</context_tokens>` hint
- [x] 5.2.2 实现 token hint 动态更新：tool registry 查询时填入当前 context token 数（±100 tokens 精度）
- [x] 5.2.3 在 tool orchestrator 注册 `compact_context` + dispatch 到 compaction pipeline + `CompactionAnalyticsAttempt.trigger = "tool-call"`
- [x] 5.2.4 tool_result 内容：返回 "Compacted N messages, freed M tokens" 摘要
- [x] 5.2.5 保留 auto-trigger 兜底：80% threshold 超越时自动触发，`trigger = "auto-threshold"`，LLM 调用不 disable auto-trigger
- [x] 5.2.6 同 iter 去重：LLM 已调用且 auto-trigger 同时触发时，只跑 LLM 调用，trigger 记为 "tool-call"
- [x] 5.2.7 新增测试：tool 注册 + reason 参数 + token hint 精度 + dispatch + auto-trigger 80% + LLM 调用后 auto-trigger 仍触发 + 同 iter 去重
- [x] 5.2.8 运行 `cargo test -p synthia-agent -p synthia-guardian -p synthia-context -p synthia-tool` + `cargo clippy` 验证阶段 5 改动

## 6. 总体验收

- [x] 6.1 运行 `cargo +nightly fmt --all` 确保代码格式统一
- [x] 6.2 运行 `cargo clippy --all-targets --all-features --tests --all` 确保零警告零错误
- [x] 6.3 运行 `cargo test --workspace --all-features` 确保所有测试通过
- [x] 6.4 运行 `openspec validate --strict` 确保 spec 格式正确
- [x] 6.5 验证无新增第三方依赖：`cargo tree -d` 确认无新 duplicate + 确认 tokio::sync::Mutex / Arc::ptr_eq / std::ops::ControlFlow 均为 stdlib/tokio 已有
- [x] 6.6 验证 otel feature 默认禁用：`cargo build` 不启用 otel + `cargo build --features otel` 启用正常
- [x] 6.7 验证 SYNTHIA_OTLP_ENDPOINT scheme 切换：`grpc://` / `http://` / 无 scheme 三种情况 exporter 选择正确
- [x] 6.8 验证 SystemContext 不 tool 化：grep tool registry 无 `update_system_context` / `set_system_context` 等
- [x] 6.9 验证 Statsig 完全剥离：grep 全 crates 无 `statsig` 符号 + `Cargo.toml` 无 statsig 依赖
