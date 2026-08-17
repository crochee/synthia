## Why

仓库根目录的 `crates/synthia-core/src/pbac/` 是一个**完整的、设计良好的 Policy-Based Access Control（PBAC）IAM 抽象**，包含 23 个 .rs 文件、1581 行代码（含 tests）。然而跨 crate 多关键词交叉搜索（`use crate::pbac` / `use synthia_core::pbac` / `AccessRequest` / `PolicySet` / `StandardRiskEvaluator` / `ConsoleAuditLogger` / `Policy::` 排除同名噪音 / `PolicyEngine` 等）结果：

| 搜索目标 | 跨 crate 命中（不含 `pbac/` 自身） |
|---|---:|
| `use crate::pbac` / `use synthia_core::pbac` | **0** |
| `synthia_core::pbac` | **0** |
| `AccessRequest` / `PolicySet` / `StandardRiskEvaluator` / `ConsoleAuditLogger` | **0** |
| `Policy::`（已过滤 CachePolicy/SanitizationPolicy/ForkPolicy 同名噪音） | **0** |
| `PolicyEngine` | **0** |
| `synthia-agent` 全文 grep `pbac` | **0** |
| `synthia-server` 全文 grep `pbac` | **0** |
| `synthia-web/src` 全文 grep `pbac` | **0** |
| `CHANGELOG.md` / `AGENTS.md` / `CLAUDE.md` 提 `pbac` | **0** |

PBAC 是 `synthia-core` 内部接口，无 vendored / patch 注入的反向依赖路径（`a2a-pb` 也不消费 `synthia_core::pbac`），且 synthia 的真实访问控制走 `synthia-tool::permission::{Permission, ToolGuard}` 自有抽象（前后几轮 archive 已闭环 `permission-fail-closed` / `permission-always-persist` 等）。**PBAC 是一个完整但未接线的 IAM 抽象**，不属于 MVP 路径，长期占用编译图与 cognitive load。

同期 `2026-08-01-post-cleanup-residue-and-rescan` 提案落盘的 `scan-results.md` 记录了一组 `cargo machete --with-metadata` 报告的"疑似未用依赖"，由本提案一并处理（**逐项交叉确认后才移除**，不盲从 machete）：

| 候选 dep | 所在 crate | 类别 | 风险 |
|---|---|---|---|
| `a2a-pb` | `synthia-server` + 根 workspace patch | runtime + patch.crates-io | 中（patch 注入，machete 误报不适用） |
| `tokio-tungstenite` | `synthia-cli` / `synthia-server` | runtime | 中（server 暴露 axum WS，需确认无引用再删） |
| `nix` | `synthia-agent` | runtime | 中（sandbox 路径，可能仅在 cfg(target_os) 下引用） |
| `libc` | `synthia-tool` | runtime | 低（无 std 替代场景通常显式声明） |
| `serial_test` | `synthia-provider` / `synthia-server` | dev-dep | 低 |
| `pretty_assertions` | `synthia-protocol` | dev-dep | 低 |

> 本提案**不安装 `cargo-udeps`**（本机不可用，避免引入新安装步骤），仍以 `cargo machete` + 手工 `grep` 交叉确认。

## What Changes

### 1. 删除 PBAC 模块（路径 1）
- **DELETE**：`crates/synthia-core/src/pbac/` 整个目录（23 个 .rs / 1581 行）
- **MODIFY**：`crates/synthia-core/src/lib.rs` 移除两行
  - 第 7 行：`pub mod pbac;`
  - 第 44 行：`pub use pbac::*;`
- **影响面**：仅 `synthia-core` crate 内部，0 跨 crate 改动，0 spec 改动
- **风险**：极低（已多关键词验证 0 调用方）

### 2. 移除 unused deps（路径 2 加项，逐项交叉确认后）
- **MODIFY**：`crates/synthia-server/Cargo.toml`：
  - 删除 `a2a-pb = "0.2"`（runtime）
  - 删除 `tokio-tungstenite.workspace = true`（runtime）
  - 删除 `serial_test = "3"`（dev-dep，确认 `#[serial]` 标注无残留）
- **MODIFY**：`crates/synthia-cli/Cargo.toml`：
  - 删除 `tokio-tungstenite = { workspace = true }`（确认 REPL 无 WS client）
- **MODIFY**：`crates/synthia-agent/Cargo.toml`：
  - 删除 `nix = { version = "0.29", features = ["signal", "process"] }`（确认 sandbox 路径无 nix-only 调用）
- **MODIFY**：`crates/synthia-tool/Cargo.toml`：
  - 删除 `libc = "0.2"`（确认无 unsafe block 显式 FFI）
- **MODIFY**：`crates/synthia-provider/Cargo.toml`：
  - 删除 `serial_test = "3"`（dev-dep）
- **MODIFY**：`crates/synthia-protocol/Cargo.toml`：
  - 删除 `pretty_assertions = "1"`（dev-dep）
- **MODIFY**：根 `Cargo.toml`：
  - 删除 `[workspace.dependencies]` 中的 `tokio-tungstenite = "0.24"`（如所有 crate 都已移除）
  - 删除 `a2a-pb` patch 块（`[patch.crates-io]` 第 104-107 行），**但保留** `vendor/a2a-pb/` 目录（archive 历史已记录）

### 3. tasks.md 强制约束（按 AGENTS.md / rust.md）
- 验证步骤必须按模块分批（`cargo test -p <module>`），**绝对禁止** `cargo test --workspace`
- clippy / fmt / 16 个 crate 单独跑
- 每条 AC 必须有可复现命令 + 期望输出
- 明确"不做什么"清单

## Capabilities

### Removed Capabilities
- `pbac`: Policy-Based Access Control 抽象（曾预留为多策略 IAM 框架，当前 0 调用方，不在 MVP 路径）

### Modified Capabilities
无（本提案仅删除文件 + 移除 unused deps，不改任何 source-of-truth spec）

### New Capabilities
无

## Impact

- **代码结构**：
  - 净减 ~1581 行 `synthia-core/src/pbac/*`（含 unit tests）
  - 净减 ~10 行 Cargo.toml（4 个 runtime + 3 个 dev-dep + workspace + patch）
  - `Cargo.lock` 自动同步（约 N KB dep metadata 减少）
- **行为变更**：无（PBAC 与 unused deps 均为 0 调用方）
- **测试**：必须保持 `cargo test -p <each-crate>` 全绿（按 AGENTS.md 分批 16 个 crate）
- **clippy / fmt**：必须保持 0 warning / 0 diff
- **`make dev`**：必须不破（synthia-server boot + synthia-web 5173）
- **`target/`**：**不**触发 `cargo clean`（15 GB `target/` 留给用户按需；本提案不动）

## Acceptance Criteria (硬性)

变更完成后必须全部通过：

| ID | 验证 | 期望 |
|---|---|---|
| AC-1 | `find crates/synthia-core/src/pbac -type f` | `0` 或 `No such file or directory` |
| AC-2 | `grep -rn 'pub use pbac\|pub mod pbac' crates/synthia-core/src/` | `0` 命中 |
| AC-3 | `grep -rn 'use synthia_core::pbac\|use crate::pbac' crates/ synthia-web/ test-support/` | `0` 命中 |
| AC-4 | `cargo +nightly clippy --all-targets --all-features --tests --all` | exit 0，0 warning |
| AC-5 | `cargo +nightly fmt --all -- --check` | exit 0，0 diff |
| AC-6 | `cargo +nightly fmt --all`（同步格式） | exit 0 |
| AC-7 | `cargo test -p synthia-core` ... `cargo test -p synthia-a2a`（按 AGENTS.md 分批 16 个 crate） | 每个 crate exit 0 |
| AC-8 | `grep -rn 'pbac' openspec/changes/` | 仅 `archive/` 历史命中，本提案目录 0 命中（除标题外），`2026-08-01-post-cleanup-residue-and-rescan/` 0 命中 |
| AC-9 | `git status` 最终 | clean（含本提案目录作为唯一 untracked） |
| AC-10 | `cargo build`（debug） | exit 0 |
| AC-11 | `cargo check --workspace --all-targets` | exit 0（确认 deps 移除未破编译图） |
| AC-12 | `make dev` dry-check（仅启动 server + web，等同前一轮 archive AC-11） | synthia-server boot 成功 + synthia-web dev server 5173 监听 |
| AC-13 | deps 移除交叉确认清单（任务步骤 1.x）已记录在 `tasks.md` | 见 tasks.md 步骤 1.1-1.6 |
| AC-14 | 不修改 `archive/`、`.trae/`、`.omo/`、`vendor/a2a-pb/`、`config.yaml` / `clippy.toml` / `deny.toml` / `rust-toolchain.toml` / `.gitignore` / `rustfmt.toml` / `CHANGELOG.md` | `git status --porcelain` 对这些路径输出空 |

**AC-1 / AC-2 / AC-3 是 PBAC 移除证据的核心闸门**：删除 + re-export 移除 + 0 调用方确认。
**AC-4 / AC-5 / AC-6 / AC-7 是健康度回归闸门**：删除后所有 clippy / fmt / 16 个 crate test 必须保持原状态。
**AC-8 是文档自洽闸门**：变更目录内不残留 PBAC 描述（除标题），仅 archive 命中。
**AC-10 / AC-11 / AC-12 是 deps 移除闸门**：证明移除 deps 后编译图、运行行为不变。
**AC-14 是边界约束闸门**：本提案仅动 `crates/synthia-core/src/pbac/` 与涉及 crate 的 `Cargo.toml`，不动其他文件。

## Out of Scope（不做项）

- ❌ **不动 `Permission` / `ToolGuard` 系统**（与 PBAC 是两个独立抽象，前几轮 archive 已闭环 `permission-fail-closed` / `permission-always-persist`）
- ❌ **不动同名 Policy 类型**：`CachePolicy`（synthia-context/service.rs）/ `SanitizationPolicy`（synthia-tool/truncate/*）/ `ForkPolicy`（synthia-agent/control/fork_policy.rs）/ `reqwest::redirect::Policy`（synthia-tool/builtin/web.rs）——这些是字面同名但与 PBAC 无关
- ❌ **不动 `RiskEvaluator` / `AuditLogger` trait 化**（PBAC 已 0 调用方，整模块移除即可）
- ❌ **不动 `vendor/a2a-pb/` 目录**（archive 历史已记录该 vendored patch 的设计意图；移除 a2a-pb 仅是移除 synthia-server 的引用，不删 vendored 目录）
- ❌ **不动 archive 历史中的 PBAC 评测记录**（`openspec/changes/archive/2026-06-15-trait-abstraction-review/` 等）
- ❌ **不动 `.trae/` / `.omo/` / `CHANGELOG.md`**（边界外）
- ❌ **不安装 `cargo-udeps`**（本机不可用，避免引入新安装步骤）
- ❌ **不重跑 mvp-smoke E2E**（前一轮 archive AC-11 已 PASS，本提案不重测端到端）
- ❌ **不触发 `cargo clean`**（15 GB `target/` 留给用户按需）
- ❌ **不主动 git push / 不创建 PR**（按 AGENTS.md）
- ❌ **不重命名/合并 workspace member crate**
- ❌ **不修改 `Cargo.lock`（除 `cargo build` 自动同步外手工不碰）**