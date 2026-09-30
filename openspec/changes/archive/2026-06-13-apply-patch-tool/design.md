## Context

Synthia 当前 `synthia-tool` 提供 7 个内置工具（read / write / multi_edit / glob / grep / path / web），
文件编辑类（read / write / multi_edit）走的是 Synthia 自己的 find-and-replace 协议。
与 opencode / codex / Claude Code 对比，缺少 Anthropic 官方 V4A `apply_patch` 工具。

**当前问题**：
- LLM 使用 Synthia 工具时必须把"我想改这一段"翻译成"old_str + new_str"两次往返
- 增加 prompt token 消耗和出错概率（context 漂移导致 old_str 不匹配）
- 多文件修改无原子性保证（多次 write 调用中间失败会留半完成状态）
- 没有"重命名"或"创建新文件"原子操作

**约束**：
- 不得破坏现有 `multi_edit` 工具（skill 测试已使用，避免回归）
- 必须复用现有路径安全（`check_path_safety`）和并发控制（`is_concurrency_safe`）抽象
- 不得引入新依赖（V4A 解析是纯文本处理，标准库足够）

**利益相关方**：
- Agent 工具调用方（LLM）：获得更符合 Anthropic 习惯的协议
- 终端用户：减少 token 消耗、降低错误率
- Guardian 审批流：复用现有 write policy，无新审批点

## Goals / Non-Goals

**Goals:**
- 实现 Anthropic V4A 规范的完整解析（`*** Begin Patch` / `*** End Patch` 标记 + 4 种头 + 上下文/diff 行）
- 支持 `Update File:` / `Add File:` / `Delete File:` / `Move to:` 四种 op
- 跨多文件原子性：snapshot 优先 + 内存 dry-run + 顺序 commit，失败回滚
- 与现有 tool 生态无缝集成（registry / permission / concurrency）
- 5 个核心测试 case 覆盖 happy path + 错误回滚

**Non-Goals:**
- **V4A 之外的格式**（unified diff / JSON Patch）：留作未来扩展
- **patch 压缩 / 编码优化**：V4A 是文本格式，不做二进制 patch
- **自动 fuzzy match**：hunk 不匹配立刻失败，不做 Levenshtein 距离兜底
- **替换 multi_edit**：保留并存，multi_edit 给简单场景用
- **修改 LLM provider 协议**：apply_patch 是 tool 实现，provider 协议层不动
- **新增对 V4A 之外 format 的检测/自动转换**

## Decisions

### D1: 范围 — 全量 V4A 范本
- **选择**：实现 `Update File:` / `Add File:` / `Delete File:` / `Move to:` 四种 op 头 + `*** End of File` 标记
- **理由**：Anthropic 官方 V4A 规范边界 case 多（End of File 标记、MOVE 在 UPDATE 之后等），半实现会遇到 LLM 输出规范内但实现不支持的尴尬情况。实现成本可控（Move 状态机只是路径重写，多约 30 行）
- **已考虑 alternative**：
  - 只 Update：占 Claude Code 使用 ~80%，但 20% 场景仍要 fallback 到 write/multi_edit
  - Update + Add + Delete：覆盖率 ~95%，但 Move 操作需要先 Delete 再 Add 不是原子

### D2: 顺序应用 — V4A 规范语义（部分失败保留已应用）
- **选择**：Parse → Resolve paths → `check_path_safety` → Permission check → Sequential apply（无 snapshot / 无 dry-run 状态机）
- **理由**：
  - **codex 实测**：scenario 015 `failure_after_partial_success_leaves_changes` 显式测试"部分成功后失败会留下已应用变更"——codex 故意不实现 atomic rollback
  - **opencode 实测**：apply-patch.ts 第 59 行 description 显式说明 "atomic rollback are not supported yet"
  - 业界共识：跨 hunk atomic rollback 的工程复杂度（snapshot 状态机、临时文件管理、错误恢复路径）远高于其价值——LLM 拿到完整的 applied/failed 列表后可以重新规划 patch
  - 单 hunk 内部已经是"要么全成要么全不成"（上下文匹配 + 行级 diff），足以覆盖 99% 的失败场景
- **已考虑 alternative**：
  - Snapshot + dry-run + commit + rollback：被 codex/opencode 双方否决，YAGNI
  - Copy-on-Write 快照：依赖 COW 文件系统，跨平台不可靠
- **影响**：失败时返回 `AppliedFailure { applied: Vec<PatchOp>, failed: (PatchOp, Reason) }`，LLM 收到后看到完整状态

### D2.5: Move hunk — 解析期接受 / 运行期默认禁用
- **选择**：grammar 接受 `*** Move to:` 行（保持 V4A 协议兼容），但默认配置 `enable_move = false`，运行时返回 `ToolFailure("apply_patch moves are not supported yet")`
- **理由**：
  - **opencode 模式**：第 85 行显式拒绝 `if (move) return yield* new ToolFailure({ message: "apply_patch moves are not supported yet" })`
  - 解析期接受意味着 LLM 不会因为语法错误失败，错误信息更可操作（"moves not enabled" vs "unexpected token"）
  - 留口子：未来启用只需翻转 config flag，无需改 grammar
- **已考虑 alternative**：
  - 在 grammar 层就拒绝 Move：破坏 V4A 协议兼容性，LLM 训练数据假设 Move 合法
  - 完全实现 Move（含跨目录）：需要先 Delete 再 Add 的原子保证，又回到 D2 atomic rollback 问题

### D3: 与 multi_edit 关系 — 并存（不替换）
- **选择**：`ApplyPatchTool` 独立实现，multi_edit 保留给简单 find-replace 场景
- **理由**：multi_edit 在 skill 测试中已被使用，替换会引入回归。apply_patch 与 multi_edit 各擅长不同场景
- **已考虑 alternative**：
  - 替换 multi_edit：破坏现有 LLM 工具调用，需要同步更新 skill 测试和 prompt

### D4: 错误恢复粒度 — 已应用保留，失败显式报告
- **选择**：Apply 阶段任一 hunk 不匹配 → 停止后续 op，**已成功应用的 op 保留不变**，返回 `AppliedFailure { applied, failed }`
- **理由**：
  - 与 D2 顺序应用决议一致
  - **codex scenario 015** 显式测试这种"部分成功 + 失败"语义
  - **opencode description** 第 59 行说明: "if a later operation fails, earlier operations remain applied and the failure reports them explicitly"
  - LLM 拿到 applied/failed 列表后可：(a) 重新规划 patch 覆盖 failed op；(b) 接受当前状态继续；(c) 用户手动介入
- **已考虑 alternative**：
  - 整体回滚（D2 原方案）：被 codex/opencode 双方否决，复杂度高、价值低
  - 部分提交但无结构化报告：LLM 不知道哪些 op 成功了，调试成本高

### D5: 权限 / Guardian 集成 — 复用 write policy
- **选择**：`requires_permission() -> true`，permission checker 复用 `write` policy
- **理由**：apply_patch 是 write 的超集（都是文件修改）。如果 write 已被 approve，apply_patch 不应重复要求审批
- **已考虑 alternative**：
  - 独立 policy：增加配置复杂度，且与"工具是 write 的特例"的语义不符
  - 不需要权限：跳过 guardian 风险高，违反现有 file mutation 工具的默认 policy

### D6: 并发安全 — 串行执行
- **选择**：`is_concurrency_safe() -> false`（与 `WriteTool` 一致）
- **理由**：mutate FS，必须串行。patch 可能跨多个文件，并发会导致 commit 顺序错乱
- **已考虑 alternative**：无（mutate FS 工具的硬性约束）

### D7: 注册位置 — `register_defaults()`
- **选择**：在 `synthia-tool/src/registry/registration.rs:110` 的 `register_defaults()` 中追加 `ApplyPatchTool`
- **理由**：不是隐藏工具（`is_hidden` = false），与 multi_edit 一样对 LLM 可见
- **已考虑 alternative**：
  - 隐藏工具（`is_hidden = true`）：除非有 opt-in 机制，否则 LLM 看不到
  - 独立 crate：单文件工具拆 crate 是过度抽象

### D8: V4A 解析器实现 — 独立模块 `v4a.rs`
- **选择**：在 `synthia-tool/src/builtin/` 下新增 `v4a.rs` 模块负责解析，应用逻辑放在 `apply_patch.rs` 调用 v4a
- **理由**：解析器（纯文本 → PatchOp Vec）与应用器（PatchOp Vec → FS）关注点分离，便于独立测试
- **已考虑 alternative**：
  - 嵌入到 `apply_patch.rs`：单文件结构简单但解析逻辑难以独立测试
  - 独立 crate：过度工程

## Risks / Trade-offs

[Risk] V4A 边缘 case 实现错误 → Mitigation: **直接复用 codex 22 个 portable scenario fixture**（`tests/fixtures/scenarios/001-022/`，每个含 `input/` + `patch.txt` + `expected/`）——codex README 明确说 "meant to be easily portable to other languages or platforms"
[Risk] 部分成功留下半完成状态让用户困惑 → Mitigation: `AppliedFailure { applied, failed }` 结构化报告，明确告诉 LLM 哪些 op 成功哪些失败；description 字段写明"operations apply sequentially; earlier operations remain applied on failure"
[Risk] Move 行为不一致 → Mitigation: D2.5 决议 — 解析期接受 / 运行期默认禁用，配置 flag 控制
[Risk] permission policy 误配置导致 LLM 跳过审批 → Mitigation: 显式 `requires_permission = true` + 复用 write policy 测试
[Risk] 与 multi_edit 工具描述混淆导致 LLM 选错 → Mitigation: description 字段明确写"V4A multi-file sequential apply" + parameters schema 清晰区分
[Trade-off] 部分成功而非原子回滚 → 接受理由：与 codex scenario 015 + opencode description 一致，业界共识
[Trade-off] 解析器代码量 ~200 行，单测覆盖度高 → 接受理由：测试成本换 LLM 协议稳定性
[Trade-off] apply_patch 与 multi_edit 并存增加 prompt 长度 → 接受理由：Apply Patch 工具描述 < 100 tokens，影响微乎其微

## Migration Plan

**N/A — 本 change 不涉及部署变更**：
- 纯加内置工具（`synthia-tool/src/builtin/apply_patch.rs` + `synthia-tool/src/builtin/v4a.rs`）
- 复用现有 tool registry / permission / concurrency 抽象
- 无 DB 变更、无 endpoint 变更、无数据迁移
- 部署只需 `cargo build -p synthia-tool && cargo install synthia-tool`

**回滚策略**：
- 若 apply_patch 在生产暴露问题，从 `register_defaults()` 移除一行即可禁用
- 旧 LLM 工具调用 fallback 到 multi_edit / write

**验收条件**：
- `cargo +nightly fmt` 无 diff
- `cargo clippy --all-targets --all-features --tests -p synthia-tool` 无新增 warning
- `cargo test -p synthia-tool` 全部通过（含 22 个 codex scenario + 6 个 v4a 单元测试 + 4 个集成测试 = 32 个新测试）
- `cargo test -p synthia-agent` 全部通过（确认 apply_patch 注册不影响其他工具）

## Open Questions

1. **V4A 行尾处理**：CRLF vs LF 兼容性 — 若 LLM 传 CRLF hunk，解析器是否要 normalize？
   - 决议：暂按 LF 处理，CRLF 报错（strict mode）。后续根据 LLM 行为调整
2. **大 patch 的内存占用**：V4A 单 hunk 解析是流式的（无需全文件读入内存），单 op 失败不影响其他 op
   - 决议：暂不设内存上限，每 op 失败有结构化报告
3. **path traversal 防护在 Move 中的位置**：Move 后的路径是否也要走 `check_path_safety`？
   - 决议：是，Move 后的目标路径也必须通过路径安全检查
4. **22 个 codex scenario 的 fixture 引入方式**：
   - 选项 A：`include_str!("fixtures/codex/015_failure_after_partial_success_leaves_changes/patch.txt")` 硬编码（编译期固定）
   - 选项 B：运行时从 `tests/fixtures/codex/` 读取（更灵活但需要构建系统配合）
   - 决议：选项 A（更简单、CI 友好、scenario 不会被运行时修改）
5. **D2.5 启用 Move 的触发条件**：
   - 当前禁用。启用需要：(a) D2 atomic rollback 重新设计、(b) 跨文件系统 mv 的兼容性测试、(c) Guardian policy 扩展
   - 决议：暂不启用，6 个月后再评估
