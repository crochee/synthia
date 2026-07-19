<!--
Raw capture of superpowers:brainstorming output.

本檔原樣捕捉 brainstorming skill 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorm: Remove cache_breaker (P0-1)

## 背景

### 来源
- **项目记忆 P0 清单**：`P0-1 移除 cache_breaker（~30 行删除）：反 P1 模式，prompt_cache_key 已做命名空间隔离`
- **多专家对抗性分析结论 (2026-06-25)**：列为 P0 立即修复项

### 现状探查
代码位置：`crates/synthia-context/src/system_context.rs`

当前 `SystemContext` 结构包含一个 `cache_breaker: String` 字段，由 `generate_cache_breaker()` 生成形如 `cb_<8-hex>` 的随机字符串。该字段在每次 `get_system_context()` 调用时（缓存未命中情况下）重新生成。

**关键发现**：
1. `cache_breaker` 字段仅在 `system_context.rs` 内部使用（grep 全仓库无外部引用）
2. `generate_cache_breaker()` 使用 `rand::thread_rng()` 产生随机值 —— 每次生成不同
3. 该字段从未被序列化进 prompt，也未作为 LLM API 的 cache key 参数
4. `SystemContext::new()` 的唯一调用点就在本文件内

### 问题诊断
这违反了 `agent_rule.md` 中的 **P1 原则（KV Cache 前缀一致性）**：

> 连续 API 调用之间，prompt 的前缀必须保持字节级一致。
> 违反它的代价是 10 倍成本 + 推理系统 I/O 瓶颈。

`cache_breaker` 的设计意图（从命名推断）是"打破缓存"以强制刷新，但这与 P1 原则直接对立。在 v2 架构中，缓存命名空间隔离已由 `prompt_cache_key`（含 `user_id` 命名空间，见 P0-4）和 `applyCachePolicy`（见 P0-2，已实现于 `kv-cache-policy-injection` change）负责。`cache_breaker` 是冗余且有害的旧机制。

更严重的是，`generate_cache_breaker()` 使用随机数 —— 即使在 TTL 缓存未命中时重新生成，也会导致两次相邻调用产生不同的 `SystemContext`，进一步破坏前缀稳定性。

## 决议链

### Q1: 是否应该完全移除 `cache_breaker`？

**选项分析**：
- **A. 完全移除**：删除字段、`generate_cache_breaker()` 函数、相关测试。代码减少 ~30 行。
- **B. 保留但改为确定性生成**（基于 git branch/status 哈希）：避免随机性，但仍保留"打破缓存"语义。
- **C. 保留字段但废弃使用**（标记 `#[deprecated]`）：向后兼容，逐步移除。

**裁决**：**A. 完全移除**。
- 记忆明确指出"prompt_cache_key 已做命名空间隔离"，`cache_breaker` 的功能已被替代。
- grep 确认无外部引用，移除是干净的。
- 保留废弃字段（C）违反 CLAUDE.md "Avoid backwards-compatibility hacks" 原则。
- 改为确定性生成（B）仍是无用的冗余抽象，违反 "Don't create helpers... for one-time operations"。

### Q2: 移除后 `SystemContext::new()` 的签名如何处理？

**现状**：`pub fn new(cache_breaker: String) -> Self` —— 强制传入 cache_breaker。

**选项**：
- **A. 改为 `pub fn new() -> Self`**（无参数）
- **B. 改为 `Default` 实现**：`impl Default for SystemContext`
- **C. 保留参数但忽略**：向后兼容

**裁决**：**A. 改为无参数 `new()`**。
- `new()` 的唯一调用点在本文件内，签名变更影响范围可控。
- 无参数 `new()` 更符合语义（无外部输入需要）。
- 不使用 `Default` 因为字段都是私有的且需要构造逻辑（git 信息通过 `get_system_context()` 填充）。

### Q3: `generate_cache_breaker()` 和 `rand` 依赖如何处理？

**现状**：`generate_cache_breaker()` 使用 `rand::Rng` 和 `rand::thread_rng()`。

**裁决**：完全删除 `generate_cache_breaker()` 函数。
- `rand` 依赖是否移除？需检查 `Cargo.toml` 是否还有其他用途。若无其他用途，移除依赖；若有，保留。

### Q4: 相关测试如何处理？

**现状**：
- `test_system_context_new`：断言 `cache_breaker` 字段值
- `test_cache_breaker_format`：测试 `generate_cache_breaker()` 输出格式

**裁决**：
- 删除 `test_cache_breaker_format`（测试的函数已删除）
- 修改 `test_system_context_new`：移除对 `cache_breaker` 的断言，改为 `new()` 无参数调用
- 修改 `test_system_context_git_accessors`：同样改为 `new()` 无参数
- 保留 `test_clear_cache`（与 cache_breaker 无关）

### Q5: 是否需要在移除后添加替代机制？

**裁决**：**不需要**。
- 缓存命名空间隔离由 `prompt_cache_key`（P0-4 处理）负责。
- 缓存策略由 `applyCachePolicy`（P0-2 已实现）负责。
- `SystemContext` 的职责仅是提供 git 环境信息（branch、status），不应承担缓存控制职责。

## 设计取捨

### 取舍 1: 简单删除 vs. 重构 SystemContext 职责
- **选择简单删除**：仅移除 `cache_breaker`，不重构 `SystemContext` 的其他部分。
- **理由**：CLAUDE.md "Surgical Changes" —— "Touch only what you must"。git_branch/git_status 字段工作正常，无需触动。

### 取舍 2: 是否同步移除 `rand` crate 依赖
- **决策**：先检查 `Cargo.toml`，若 `rand` 仅被 `generate_cache_breaker` 使用，则移除依赖；否则保留。
- **理由**：避免留下 unused dependency（Rust 规范要求清理 unused 代码）。

### 取舍 3: 是否更新 CHANGELOG 或文档
- **决策**：不主动创建文档（遵循 CLAUDE.md "NEVER proactively create documentation files"）。
- 若有现有文档提及 `cache_breaker`，更新之；否则不动。

## 风险评估

### 低风险
- **影响范围**：`cache_breaker` 仅在 `system_context.rs` 内部使用，无外部 API 暴露。
- **无行为变化**：`cache_breaker` 从未被序列化或用于 LLM 请求，移除不影响运行时行为。
- **测试覆盖**：现有测试可直接修改，无需新增测试（移除字段不需要新测试验证"它不存在"）。

### 验证标准
1. `cargo check -p synthia-context` 编译通过
2. `cargo test -p synthia-context` 全部通过
3. `cargo clippy -p synthia-context --all-targets --all-features --tests` 无警告
4. `cargo +nightly fmt --all --check` 格式正确
5. 全仓库 grep `cache_breaker` 无结果
