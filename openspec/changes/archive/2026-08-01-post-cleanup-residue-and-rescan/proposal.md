## Why

前一轮 OpenSpec 变更 `2026-08-01-deep-cleanup-mvp-aligned-workspace`（已在 `openspec/changes/archive/` 中归档）已经完成 MVP 工作区对齐、`cargo tree` 与 `grep` 基线采集、所有结构性清理任务的执行与 E2E 贯通（AC-11 mvp-smoke PASS）。但 **该变更过程中产生的 4 个基线/快照文件未被自动收尾**，目前仍躺在仓库根目录：

| 文件 | 大小 | 内容 |
|---|---:|---|
| `cleanup-baseline-e2e.txt` | 2 104 B | Layer 1.0.6 记录的 E2E 贯通基线结果（PASS / wall-clock / 端口可达性） |
| `cleanup-baseline-readme.txt` | 0 B | 计划留作 README diff 占位（实际未写入） |
| `cleanup-baseline-tombstones.txt` | 29 215 B | Layer 1.2 跑 `grep -rn 'synthia-' crates/ synthia-web/ tests/` 的全文输出 |
| `cleanup-baseline-tree.txt` | 64 701 B | Layer 1.1 跑 `cargo tree -p synthia-server` 的依赖树快照 |
| **合计** | **~95 KB** | |

**当前状态（已 `git ls-files` + `git log` + grep 确认）**：
- 这 4 个文件**在 git 中是 tracked 的**（在 `c2096005 chore: remove tombstone comments referencing deleted crates` 这同一批 commit 中被 `git add` 提交了，与 archive 提案的 tombstone 清理动作一起进库）
- `git ls-files cleanup-baseline-*.txt` → 4 行命中；`git log --oneline --all -- cleanup-baseline-tree.txt` → `c2096005`
- `git status` 报 `nothing to commit, working tree clean`（说明所有文件已 committed，无未提交改动）
- 全仓 `grep -rn 'cleanup-baseline'` 命中 6+42=48 处：6 处位于已归档的 `openspec/changes/archive/2026-08-01-deep-cleanup-mvp-aligned-workspace/{design,tasks}.md`（作为执行记录），**其余 42 处全在 `openspec/changes/2026-08-01-post-cleanup-residue-and-rescan/`**（即本提案自身的三件套 + scan-results.md）— 没有脚本 / CI / Makefile 引用这些文件
- 归档 proposal / design / tasks 中已完整固化所有基线数据（`tasks.md` Layer 1.0-1.2 明确记录了 `cleanup-baseline-e2e.txt` 的 PASS 结果与 wall-clock，Layer 1.1 记录 `cargo tree` 的归档位置），**不会丢信息**

**前一轮 archive 提案在归档时（`c2096005` 提交）只清理了 tombstone 注释 + 把 `cargo tree` 等 baseline 输出文件一并 `git add` 进了 commit**，但 archive 流程里没有"清场"步骤把它们删除。这些 4 个 .txt 实质上是 archive 提案 Phase 1 baseline 步骤的执行证据，对长期维护没有信息增量（`cargo tree` 输出可随时从 `Cargo.lock` 派生，tombstone grep 列表已在 Layer 5 处理完毕，e2e PASS 数字已写入 archive tasks.md）。

**本提案同步做一次轻量新扫描**，运行 `cargo +nightly clippy --all-targets --all-features --tests --all` 与 `cargo +nightly fmt --all --check` 与 `cargo machete`（`cargo-udeps` 在本机不可用），把"自 archive 之后新冒出的死代码 / dead refs / unused imports / 重复模块"列出来作为本提案的副产物 — 提案**不强制处理全部发现项**（避免改动面爆炸），只输出**清单 + 优先级建议**留给后续变更。

## What Changes

**1. 用 `git rm` 删除 4 个 tracked 的 `cleanup-baseline-*.txt` 残留文件**
- From: 仓库根 4 个 tracked `.txt`，共 ~95 KB（`c2096005` commit 提交）
- To: 用 `git rm` 从 index + working tree 一并移除
- Reason: 历史基线已固化在前一轮 archive proposal/design/tasks 中；这些文件无脚本/CI 引用、`cargo tree` 可从 `Cargo.lock` 派生、tombstone grep 列表已处理完毕、e2e PASS 数字已写入 archive，纯仓库体积残留
- Impact: 仓库根 `ls` 输出变干净；`grep -rn 'cleanup-baseline'` 在源树中只剩 archive 内的 6 处历史记录

**2. 轻量新扫描（只读副产物，不在本提案内强制改码）**
- 运行 `cargo +nightly clippy --all-targets --all-features --tests --all`（已跑过，2m 52s，0 warning）
- 运行 `cargo +nightly fmt --all -- --check`（已跑过，clean）
- 运行 `cargo machete --with-metadata`（已跑过，输出若干疑似未用依赖，附在 `design.md` 第 4 节）
- `cargo-udeps` 在本机不可用（`cargo --list` 无 `udeps` 子命令） — 在 tasks.md 中标注"未执行"及原因
- **不在本提案内修改任何 .rs / Cargo.toml** — 仅把扫描结果落盘为 `openspec/changes/2026-08-01-post-cleanup-residue-and-rescan/scan-results.md`，由后续提案决定如何处理
- Impact: 0 行代码变更；为后续清理提供决策依据

**3. tasks.md 强制约束（按 AGENTS.md / rust.md 规范）**
- 验证步骤必须按模块分批（`cargo test -p <module>`），**绝对禁止** `cargo test --workspace`
- 任何步骤涉及的 clippy / fmt / machete 输出必须**可复现**（命令 + 期望 exit code + 关键输出片段）
- 不重跑 mvp-smoke E2E（用户明确未要求复跑）
- 全部改动后 `git status` 仍需保持 clean（因为删除的是 untracked 文件，删除后 working tree 仍 clean）

## Capabilities

### New Capabilities
- `post-cleanup-residue-hygiene`: 显式收尾前一轮 archive 提案遗留在工作区的 baseline 文件，避免对后续贡献者产生"这是什么的 .txt"困惑
- `lightweight-rescan-output`: 提供 `scan-results.md` 作为后续清理变更的事实底座

### Modified Capabilities
无（本提案仅删除文件 + 落盘扫描清单，不改任何 source-of-truth spec）

## Impact

- **仓库根整洁度**: 4 个 .txt 残留删除，`ls` 输出干净
- **代码结构**: 0 行代码 / Cargo.toml 变更
- **测试**: 必须保持 `cargo test -p <each-crate>` 全绿（按 AGENTS.md 分批）
- **clippy / fmt**: 必须保持 0 warning / 0 diff（已在新扫描中确认）
- **行为变更**: 无
- **破坏性**: 无（删除的是 untracked 文件，不影响任何 tracked 路径）
- **`target/`**: **不**触发 `cargo clean`（15 GB 留给用户按需；本提案不动）

## Acceptance Criteria (硬性)

变更完成后必须全部通过：

| ID | 验证 | 期望 |
|---|---|---|
| AC-1 | `git status`（删除前） | `nothing to commit, working tree clean`（4 个文件是 tracked、committed 状态，无未提交改动） |
| AC-2 | `git rm cleanup-baseline-*.txt` 后 `ls /home/crochee/workspace/synthia/cleanup-baseline-*.txt` 2>/dev/null \| wc -l | `0` |
| AC-3 | `git status`（删除 + `git add -A` stage 后） | `Changes to be committed: deleted: cleanup-baseline-e2e.txt ...`（4 个 deletion 已 stage，等待 commit） |
| AC-3b | `git commit` 后 `git status` | `nothing to commit, working tree clean`（commit 后 working tree 恢复 clean） |
| AC-4 | `cargo +nightly clippy --all-targets --all-features --tests --all` | exit 0，0 warning |
| AC-5 | `cargo +nightly fmt --all -- --check` | exit 0，0 diff |
| AC-6 | `cargo +nightly fmt --all`（同步格式） | exit 0 |
| AC-7 | `cargo test -p synthia-core -p synthia-telemetry -p synthia-provider -p synthia-hook -p synthia-context -p synthia-tool -p synthia-command -p synthia-skill -p synthia-session -p synthia-agent -p synthia-cli -p synthia-server -p synthia-job -p synthia-cache-mark -p synthia-protocol -p synthia-a2a`（按 AGENTS.md 分批） | exit 0（每个 crate 单独跑，按 tasks.md 拆 16 个子步骤） |
| AC-8 | `cargo machete --with-metadata` 输出 | 落盘到 `scan-results.md`（仅记录，不修改） |
| AC-9 | `git status` 最终 | clean |
| AC-10 | `grep -rn 'cleanup-baseline' openspec/changes/2026-08-01-post-cleanup-residue-and-rescan/` | 42 命中（描述性引用，**保留**作为变更依据） |
| AC-11 | `grep -rn 'cleanup-baseline' . --exclude-dir=target --exclude-dir=node_modules --exclude-dir=.git --exclude-dir=sessions` | 仅命中 `openspec/changes/archive/2026-08-01-deep-cleanup-mvp-aligned-workspace/{design,tasks}.md`（6 处）+ 本提案目录（42 处），无其他引用 |

**AC-1 / AC-2 / AC-3 / AC-3b 是删除证据的核心闸门**：4 个 tracked 文件 `git rm` + commit 后，从 working tree 永久消失（commit 进入 git 历史可查可回滚），`ls` 找不到任何匹配。
**AC-4 / AC-5 / AC-7 是健康度回归闸门**：删除后所有 clippy / fmt / test 必须保持原状态。
**AC-8 是新扫描副产物**：machete 输出落盘到 `scan-results.md`，不作为本提案的修改门禁。

## Out of Scope（不重跑项）

- **不复跑 mvp-smoke E2E**：用户明确未要求；前一轮 archive 已验证 AC-11 PASS
- **不跑 `cargo clean`**：15 GB `target/` 留给用户按需
- **不处理 `cargo machete` 报告的疑似未用依赖**：仅记录到 `scan-results.md`，由后续变更决策
- **不动 `cargo-udeps`**：本机不可用，不在本次安装
