## Context

`SystemContext`（位于 `crates/synthia-context/src/system_context.rs`）当前包含一个 `cache_breaker: String` 字段，由 `generate_cache_breaker()` 通过 `rand::thread_rng()` 生成形如 `cb_<8-hex>` 的随机字符串。

该字段的设计意图是"打破 LLM API 缓存"，但这与 `agent_rule.md` 的 **P1 原则（KV Cache 前缀一致性）** 直接对立：

> 连续 API 调用之间，prompt 的前缀必须保持字节级一致。违反它的代价是 10 倍成本 + 推理系统 I/O 瓶颈。

**现状约束**：
- `cache_breaker` 仅在 `system_context.rs` 内部使用，全仓库 grep 无外部引用
- `SystemContext::new()` 的唯一调用点在本文件内（`get_system_context()` 函数）
- `generate_cache_breaker()` 使用随机数，即使 TTL 缓存未命中也会重新生成不同值，加剧前缀不稳定
- 缓存命名空间隔离职责已由 `prompt_cache_key`（P0-4 处理）和 `applyCachePolicy`（P0-2 已实现）承担

**依赖异常**：`synthia-context/Cargo.toml` 未声明 `rand` 依赖，但代码使用了 `rand::Rng`。`rand = "0.8"` 在 workspace 级别 `Cargo.toml` 中存在，可能通过其他 crate 传递引入。实现时需验证并清理。

## Goals / Non-Goals

**Goals:**
- 完全移除 `SystemContext.cache_breaker` 字段及其生成函数 `generate_cache_breaker()`
- 消除违反 P1 原则的随机缓存打破机制
- 清理因移除而产生的 unused 代码和依赖（遵循 Rust 规范，不使用 `dead_code`/`unused` 标签）
- 修改相关测试以适配新签名

**Non-Goals:**
- 不重构 `SystemContext` 的其他字段（`git_branch`、`git_status`、`beta_headers`）—— 遵循 Surgical Changes 原则
- 不实现替代的缓存控制机制 —— 已由 P0-2 (`applyCachePolicy`) 和 P0-4 (`prompt_cache_key`) 负责
- 不修改 `get_system_context()` 的 TTL 缓存逻辑 —— 该逻辑工作正常，与 `cache_breaker` 无关
- 不更新文档或 CHANGELOG（除非现有文档提及 `cache_breaker`）

## Decisions

### D1：完全移除 `cache_breaker`（不保留废弃标记）

- **选择**：删除 `cache_breaker: String` 字段、`generate_cache_breaker()` 函数、相关测试断言
- **理由**：
  1. 记忆明确指出 `prompt_cache_key` 已做命名空间隔离，功能已被替代
  2. grep 确认无外部引用，移除是干净的
  3. CLAUDE.md 明确要求 "Avoid backwards-compatibility hacks like renaming unused _vars, re-exporting types, adding // removed comments for removed code"
- **已考虑 alternative**：
  - 保留但改为确定性生成（基于 git 哈希）—— 拒绝，仍是冗余抽象，违反 "Don't create helpers for one-time operations"
  - 标记 `#[deprecated]` 逐步移除 —— 拒绝，无外部消费者，无需渐进废弃

### D2：`SystemContext::new()` 改为无参数签名

- **选择**：`pub fn new(cache_breaker: String) -> Self` → `pub fn new() -> Self`
- **理由**：
  1. 唯一调用点在本文件内，签名变更影响范围可控
  2. 移除 `cache_breaker` 后，构造函数无需任何外部输入（git 信息由 `get_system_context()` 填充）
  3. 无参数 `new()` 语义更清晰
- **已考虑 alternative**：
  - 改用 `Default` trait —— 拒绝，字段私有且 `beta_headers` 需初始化为 `Vec::new()`，`Default` 语义不如 `new()` 明确
  - 保留参数但忽略 —— 拒绝，违反 "Avoid backwards-compatibility hacks"

### D3：删除 `generate_cache_breaker()` 函数及 `test_cache_breaker_format` 测试

- **选择**：完全删除函数和对应测试
- **理由**：
  1. 测试的目标函数已删除，测试失去存在意义
  2. 不需要新测试验证"字段不存在"（这是反向测试，无价值）
- **已考虑 alternative**：无

### D4：验证并清理 `rand` 依赖

- **选择**：实现时检查 `rand` 在 `synthia-context` crate 中是否还有其他用途。若无，从 `Cargo.toml` 移除（若存在声明）；若 `rand` 未在 crate Cargo.toml 声明（当前状态），则仅需删除代码引用
- **理由**：Rust 规范要求清理 unused 依赖，不留 `dead_code`/`unused` 标签
- **已考虑 alternative**：无

### D5：修改受影响的测试适配新签名

- **选择**：
  - `test_system_context_new`：改为 `SystemContext::new()` 无参数调用，移除 `cache_breaker` 断言
  - `test_system_context_git_accessors`：改为 `SystemContext::new()` 无参数调用
  - `test_clear_cache`：保持不变（与 `cache_breaker` 无关）
  - 删除 `test_cache_breaker_format`
- **理由**：保持测试覆盖原有逻辑（git accessor、cache 清除），仅移除与 `cache_breaker` 直接相关的断言

## Risks / Trade-offs

- [Risk] `rand` 依赖移除可能影响其他未发现的传递引用 → Mitigation: 实现时执行 `cargo check -p synthia-context` 和 `cargo build` 验证；若 `rand` 未在 crate Cargo.toml 声明，则无需移除（已是传递依赖）
- [Risk] 存在未发现的 `cache_breaker` 外部引用（如动态字符串拼接） → Mitigation: 实现前已全仓库 grep 确认无外部引用；实现后再次 grep 验证
- [Trade-off] 移除后失去"主动打破缓存"能力 → 接受理由：这正是设计目标。P1 原则要求前缀稳定，"打破缓存"是反模式；缓存控制应由 `applyCachePolicy` 显式管理，而非随机扰动

## Migration Plan

本 change 不涉及部署变更（纯代码清理，无 endpoint / DB / 配置变更）。

**部署顺序**：
1. 修改 `system_context.rs`（移除字段、函数、修改测试）
2. 验证 `rand` 依赖状态（仅在需要时清理 Cargo.toml）
3. 运行 `cargo check -p synthia-context` → `cargo test -p synthia-context` → `cargo clippy` → `cargo +nightly fmt`

**Rollback 策略**：N/A —— 纯代码删除，git revert 即可回滚。

**验收条件**：
1. `cargo check -p synthia-context` 编译通过
2. `cargo test -p synthia-context` 全部通过
3. `cargo clippy -p synthia-context --all-targets --all-features --tests` 无警告
4. `cargo +nightly fmt --all --check` 格式正确
5. 全仓库 grep `cache_breaker` 无结果（除 openspec/changes 目录下的文档）

## Open Questions

无 —— 所有决策已在 brainstorm 阶段明确，上下文充分。
