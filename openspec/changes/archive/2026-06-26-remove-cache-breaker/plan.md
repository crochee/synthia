# Remove cache_breaker Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development
> to implement this plan task-by-task.

**Goal:** 移除 `SystemContext.cache_breaker` 字段及其生成函数，消除违反 P1 原则（KV Cache 前缀一致性）的随机缓存打破机制。

**Architecture:** `SystemContext` 位于 `crates/synthia-context/src/system_context.rs`，提供 git 环境信息（branch/status）并带有 5 分钟 TTL 缓存。`cache_breaker` 是私有字段，仅在本文件内部使用，全仓库无外部引用。移除后无需替代机制——缓存命名空间隔离由 `prompt_cache_key`（P0-4）负责，缓存策略由 `applyCachePolicy`（P0-2 已实现）负责。

**Tech Stack:** Rust, Cargo workspace, `parking_lot::Mutex`（TTL 缓存）, `rand`（待移除的依赖）

---

## Task 1: 移除 cache_breaker 字段与生成函数

- [ ] **Step 1:** 打开 `crates/synthia-context/src/system_context.rs`，定位第 17 行 `pub cache_breaker: String,`，删除该行
- [ ] **Step 2:** 定位 `impl SystemContext` 块中的 `pub fn new(cache_breaker: String) -> Self`，将签名改为 `pub fn new() -> Self`，并删除函数体内的 `cache_breaker,` 字段初始化行
- [ ] **Step 3:** 定位第 67-72 行的 `fn generate_cache_breaker() -> String` 函数（含 `use rand::Rng` 和 `rand::thread_rng()`），完整删除该函数
- [ ] **Step 4:** 定位 `get_system_context()` 函数中第 49 行 `let mut context = SystemContext::new(generate_cache_breaker());`，改为 `let mut context = SystemContext::new();`
- [ ] **Step 5:** 运行 `cargo check -p synthia-context` 确认编译通过（预期无 `rand` 引用错误，因 `Cargo.toml` 未声明 `rand`）

## Task 2: 修改测试适配新签名

- [ ] **Step 1:** 定位 `test_system_context_new`（约第 118-125 行），将 `let ctx = SystemContext::new("test_breaker".to_string());` 改为 `let ctx = SystemContext::new();`，并删除 `assert_eq!(ctx.cache_breaker, "test_breaker");` 断言行
- [ ] **Step 2:** 定位 `test_cache_breaker_format`（约第 127-132 行），完整删除该测试函数
- [ ] **Step 3:** 定位 `test_system_context_git_accessors`（约第 134-142 行），将 `let mut ctx = SystemContext::new("test".to_string());` 改为 `let mut ctx = SystemContext::new();`
- [ ] **Step 4:** 确认 `test_clear_cache`（约第 144-149 行）无需修改（与 cache_breaker 无关）
- [ ] **Step 5:** 运行 `cargo test -p synthia-context` 确认全部测试通过

## Task 3: 验证与清理

- [ ] **Step 1:** 检查 `crates/synthia-context/Cargo.toml`，确认未声明 `rand` 依赖（当前状态：无 `rand.workspace = true`，预期无需修改）
- [ ] **Step 2:** 运行 `cargo clippy -p synthia-context --all-targets --all-features --tests`，确认无警告（重点关注 unused import 警告）
- [ ] **Step 3:** 运行 `cargo +nightly fmt --all` 格式化代码
- [ ] **Step 4:** 运行 `cargo +nightly fmt --all --check` 确认格式正确
- [ ] **Step 5:** 全仓库 grep `cache_breaker`，确认源码无残留（仅 `openspec/changes/remove-cache-breaker/` 文档目录有匹配）
- [ ] **Step 6:** 运行 `cargo test -p synthia-context` 最终确认所有测试通过

## Verification Commands

```bash
cargo check -p synthia-context
cargo test -p synthia-context
cargo clippy -p synthia-context --all-targets --all-features --tests
cargo +nightly fmt --all --check
```
