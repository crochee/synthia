# 设计：删除 `synthia-core/src/pbac` 模块 + 移除 unused deps

> 本文是 `2026-08-01-remove-core-pbac-and-dead-deps` 的设计说明。事实底座见 `openspec/changes/_inbox/2026-08-01-ai-agent-irrelevant-logic-cleanup/exploration.md`。

## 1. 设计原则

- **最小变更面**：仅删 PBAC 整目录 + 移除 unused deps；不动其他模块、不重构 Permission、不动 archive
- **可复现**：每条 AC 配可执行命令 + 期望输出
- **分批验证**：clippy / fmt / 16 个 crate test 单独跑（按 AGENTS.md）
- **事实驱动**：deps 移除前必须 `grep -rn '<dep>' crates/` 交叉确认 0 引用

## 2. PBAC 模块结构（删除前快照）

```
crates/synthia-core/src/pbac/
├── mod.rs                                # 模块入口，pub use 三件套
├── context/                              # 5 文件
│   ├── mod.rs
│   ├── subject.rs
│   ├── action.rs
│   ├── resource.rs
│   └── environment.rs                    # 含 AccessRequest, ContextRisk
├── evaluation/                           # 9 文件
│   ├── mod.rs
│   ├── evaluator.rs                      # StandardRiskEvaluator
│   ├── audit.rs                          # ConsoleAuditLogger
│   ├── types.rs
│   └── ...
└── policy/                               # 8 文件
    ├── mod.rs
    ├── core.rs                           # Policy trait, AsyncPolicy
    ├── set.rs                            # PolicySet
    ├── condition.rs                      # PolicyCondition, Resolve
    └── ...
```

合计 **23 个 .rs / 1581 行**（含 unit tests）。

## 3. re-export 链

```
crates/synthia-core/src/lib.rs
├── pub mod pbac;                          # 第 7 行 → 删除
└── pub use pbac::*;                       # 第 44 行 → 删除
```

删除后 `synthia-core` 公开 API 列表（`pub use` 块）需重排（删除第 44 行会让后续 `pub use registry::*;` 顶到第 44 行）。其他 11 个 `pub use` 块（api / error / filesystem / id / json_schema / path / registry / secret / text / time + 顶层 pub mod）保持不变。

## 4. unused deps 交叉确认矩阵

> 步骤 1.x 必须按此矩阵逐项跑通后才动手改 `Cargo.toml`。

| dep | 所在 crate | grep 命令 | 期望命中 | 决策 |
|---|---|---|---|---|
| `a2a-pb` | synthia-server + root patch | `grep -rn 'a2a-pb' crates/synthia-server/src/` | **0** | ✅ 删除 |
| `tokio-tungstenite` | synthia-cli | `grep -rn 'tungstenite' crates/synthia-cli/src/` | 0 | ✅ 删除（REPL 无 WS client） |
| `tokio-tungstenite` | synthia-server | `grep -rn 'tungstenite' crates/synthia-server/src/` | 0（server WS 走 axum 内置 `axum::extract::ws`） | ✅ 删除 |
| `nix` | synthia-agent | `grep -rn 'nix::' crates/synthia-agent/src/` | 0（sandbox 走 Bubblewrap / OsFileSystem） | ✅ 删除 |
| `libc` | synthia-tool | `grep -rn 'libc::\|use libc' crates/synthia-tool/src/` | 0 | ✅ 删除 |
| `serial_test` | synthia-provider | `grep -rn '#\[serial\]\|#\[parallel\]\|#\[test\]' crates/synthia-provider/src/` | 仅 #[test]，无 #[serial] | ✅ 删除 |
| `serial_test` | synthia-server | `grep -rn '#\[serial\]\|#\[parallel\]' crates/synthia-server/src/` | 0 | ✅ 删除 |
| `pretty_assertions` | synthia-protocol | `grep -rn 'pretty_assertions' crates/synthia-protocol/src/` | 0 | ✅ 删除 |
| 根 workspace `tokio-tungstenite` | Cargo.toml | 所有引用 crate 已删 → workspace 行同步删 | — | ✅ 删除 |

**如有命中**：立即停手，把发现写进 `tasks.md` "阻塞项"，本提案不破坏既有调用方。

## 5. 改动文件清单

```
crates/synthia-core/src/pbac/                             # DELETE 整个目录（23 文件）
crates/synthia-core/src/lib.rs                            # DELETE 2 行（第 7、44）
crates/synthia-server/Cargo.toml                          # DELETE 3 行（a2a-pb / tokio-tungstenite / serial_test）
crates/synthia-cli/Cargo.toml                             # DELETE 1 行（tokio-tungstenite）
crates/synthia-agent/Cargo.toml                           # DELETE 1 行（nix）
crates/synthia-tool/Cargo.toml                            # DELETE 1 行（libc）
crates/synthia-provider/Cargo.toml                        # DELETE 1 行（serial_test）
crates/synthia-protocol/Cargo.toml                        # DELETE 1 行（pretty_assertions）
Cargo.toml                                                # DELETE tokio-tungstenite workspace + a2a-pb patch 块
Cargo.lock                                                # cargo build 自动同步
```

**不动**：
```
crates/synthia-server/src/                  # 仅 Cargo.toml 改
crates/synthia-cli/src/                     # 仅 Cargo.toml 改
crates/synthia-agent/src/                   # 仅 Cargo.toml 改
crates/synthia-tool/src/                    # 仅 Cargo.toml 改
crates/synthia-provider/src/                # 仅 Cargo.toml 改
crates/synthia-protocol/src/                # 仅 Cargo.toml 改
vendor/a2a-pb/                              # vendored patch 目录保留
openspec/changes/archive/                   # 历史记录保留
openspec/changes/2026-08-01-post-cleanup-residue-and-rescan/   # 另一 active change 保留
openspec/changes/_inbox/                    # 探索笔记保留
CHANGELOG.md / .trae/ / .omo/               # 边界外
config.yaml / clippy.toml / deny.toml / rust-toolchain.toml / .gitignore / rustfmt.toml  # 工具配置不动
```

## 6. 执行顺序（tasks.md 步骤拆分）

```
阶段 1（前置确认，只读）
  1.1-1.6  deps 逐项交叉 grep（0 命中后才动手）
  1.7      PBAC 调用方最后复核（再 grep 一次）

阶段 2（PBAC 移除）
  2.1      git rm -r crates/synthia-core/src/pbac/
  2.2      改 lib.rs（删 2 行）
  2.3      cargo build -p synthia-core（确认 0 warning）

阶段 3（deps 移除）
  3.1-3.6  逐 crate 改 Cargo.toml
  3.7      改根 Cargo.toml（workspace + patch）
  3.8      cargo build（自动同步 Cargo.lock）

阶段 4（健康度回归）
  4.1      cargo +nightly fmt --all
  4.2      cargo +nightly clippy --all-targets --all-features --tests --all
  4.3-4.18 cargo test -p <each-crate>（16 个 crate 单独跑）

阶段 5（终验）
  5.1-5.14 AC-1 / AC-2 / ... / AC-14
```

## 7. 回滚预案

- 本提案**不主动 commit**（按 AGENTS.md "不主动 push"；commit 由用户决定）
- 若任一 AC 失败：保留 working tree 不 commit，定位失败 crate + 修复后从该步骤重跑
- 极端情况下 `git restore .` 一键回滚所有 staged 改动

## 8. 与其他 active change 的关系

- **`2026-08-01-post-cleanup-residue-and-rescan`**（active，未归档）：仅删 4 个 tracked `.txt` + 跑轻量扫描，**不改 `crates/` 与 `Cargo.toml`**。两个 active change **无冲突**，可同周期推进，但建议先后归档（先归档前一个，再启动本提案）
- **`_inbox/2026-08-01-ai-agent-irrelevant-logic-cleanup/`**（思考阶段）：本提案引用其 exploration.md 作为事实底座，proposal 创建后该 inbox 仍保留作为决策日志

## 9. 不做的事（重申）

见 `proposal.md` §Out of Scope。核心三点：

1. **不动 Permission / ToolGuard**（PBAC 是 IAM 框架，permission 是 tool gating，两者隔离）
2. **不动 vendored / patch 目录**（vendor/a2a-pb 保留；仅移除引用方）
3. **不盲从 machete**（每条 dep 必须手工 grep 确认 0 引用后再删）