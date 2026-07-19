## Why

`SystemContext.cache_breaker` 字段通过 `rand::thread_rng()` 生成随机字符串，意图"打破 LLM API 缓存"。这直接违反 `agent_rule.md` 的 P1 原则（KV Cache 前缀一致性），该原则要求连续 API 调用间 prompt 前缀字节级不变，违反代价是 10 倍成本 + I/O 瓶颈。缓存命名空间隔离职责已由 `prompt_cache_key`（P0-4）和 `applyCachePolicy`（P0-2 已实现）承担，`cache_breaker` 是冗余且有害的旧机制。现需移除以恢复前缀稳定性。

## What Changes

**SystemContext 结构**
- From: `SystemContext` 包含 `cache_breaker: String` 字段，由 `generate_cache_breaker()` 随机生成
- To: `SystemContext` 不再包含 `cache_breaker` 字段
- Reason: 消除违反 P1 原则的随机缓存打破机制
- Impact: non-breaking（字段仅在 crate 内部使用，无外部 API 暴露）

**SystemContext::new() 构造函数**
- From: `pub fn new(cache_breaker: String) -> Self` —— 强制传入 cache_breaker
- To: `pub fn new() -> Self` —— 无参数
- Reason: 移除 cache_breaker 后无需外部输入
- Impact: non-breaking（唯一调用点在本文件内）

**generate_cache_breaker() 函数**
- From: 存在私有函数 `generate_cache_breaker()` 使用 `rand::thread_rng()`
- To: 完全删除
- Reason: 功能已被 `prompt_cache_key` 和 `applyCachePolicy` 替代
- Impact: non-breaking（私有函数，无外部调用）

**测试**
- From: `test_cache_breaker_format` 测试存在；`test_system_context_new` 和 `test_system_context_git_accessors` 使用带参数的 `new()`
- To: 删除 `test_cache_breaker_format`；其余测试改为无参数 `new()` 调用并移除 `cache_breaker` 断言
- Reason: 适配新签名，移除已删除功能的测试
- Impact: non-breaking

## Capabilities

### New Capabilities

- `system-context`: 形式化 SystemContext 的对外行为契约（git 环境信息提供 + TTL 缓存），明确不包含缓存打破机制

### Modified Capabilities

无 —— `openspec/specs/` 目录为空，本 change 首次为 `system-context` 建立正式 spec。

## Impact

**受影响代码**：
- `crates/synthia-context/src/system_context.rs` —— 移除字段、函数、修改测试（~30 行删除/修改）

**受影响依赖**：
- `rand` crate（workspace 级 `rand = "0.8"`）—— 若移除后 `synthia-context` crate 不再使用 `rand`，且 `Cargo.toml` 中有声明则移除；当前 `Cargo.toml` 未声明 `rand`，仅需删除代码引用

**不受影响**：
- 无 API 变更（`cache_breaker` 是私有字段）
- 无 endpoint / DB / 配置变更
- 无下游 crate 影响（grep 确认无外部引用）
