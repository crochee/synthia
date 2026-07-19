# Deep Review: `Retryable`

**Location**: `crates/synthia-provider/src/retry.rs:6` (trait REMOVED 2026-06-15 in change `2026-06-15-p0-trait-review-remediation` Sub-task A)
**Signals**: 1 impl / 1 methods / 0 generics / 0 call sites / 0 dyn
**Status**: ✅ **REMOVED** (was REMOVE_CANDIDATE)

## 目的
为 `synthia_core::Error` 添加"是否可重试"能力的 marker trait。1 个方法 `is_retryable(&self) -> bool`。

## 存在价值
- 1 impl: `impl Retryable for Error { fn is_retryable(&self) -> bool { self.is_retryable() } }`
- **已验证 (2026-06-15)**: `synthia_core::Error::is_retryable` 是 inherent method (位于 `crates/synthia-core/src/error.rs:218`),Rust 方法解析优先 inherent method,line 12 的 `self.is_retryable()` **委托给 inherent 方法,非无限递归**
- 结论: trait 是**纯死包装 (no-op wrapper)**,无任何功能
- 0 dyn 引用

## 替代方案
- **A) 直接用 `Error::is_retryable()` 方法** (已经是 inherent method,见 11-14 行)
- **B) 保留 trait**: 1 方法,可由 inherent method 完全替代
- **C) 拆 trait**: 1 方法无法拆

## 推荐
**REMOVE_CANDIDATE** (移除 trait, 统一用 `Error::is_retryable()` inherent 方法)

## 理由
**这是反模式**:trait 唯一 impl 直接调用 `self.is_retryable()`,这要么是递归(死循环)要么委托给 Error 内置同名方法。后者情况下,trait 是**纯包装无任何功能**。YAGNI 反例。

## 4-party 检查

- **怀疑派**: 1 impl + 0 dyn + 委托到同名方法 = 死代码。**REMOVE_CANDIDATE**。
- **架构派**: trait 设计目的是"为不同类型提供 is_retryable",但当前仅 Error 1 类型。**REMOVE_CANDIDATE**。
- **生产派**: 移除不影响任何调用方(0 dyn)。**REMOVE_CANDIDATE**。
- **简化派**: Error::is_retryable() inherent 方法已经存在。**REMOVE_CANDIDATE**。

**共识**: 4 派一致 (4-0) — **REMOVE_CANDIDATE**。

### 紧急
- **已验证 (2026-06-15)**: `synthia_core::Error::is_retryable` 确认是 inherent method (`crates/synthia-core/src/error.rs:218`),**无递归风险**。
- 原描述"可能形成无限递归"为误判 — Rust 方法解析规则保证 `self.is_retryable()` 优先匹配 inherent method,实际是 no-op wrapper。

### 实施结果 (2026-06-15)
- Sub-task A 完成: 删除 `pub trait Retryable` + `impl Retryable for Error` (9 行)
- `cargo test -p synthia-provider`: 34 passed, 0 failed (含 `is_retryable_error(status)` 测试)
- 0 调用方迁移 (trait 0 引用,删除无影响)
- 详见 `openspec/changes/2026-06-15-p0-trait-review-remediation/verify.md`
