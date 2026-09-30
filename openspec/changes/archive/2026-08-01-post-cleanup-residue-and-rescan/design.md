## Context（思考用，不写入文件）

- 前一轮 OpenSpec 变更 `2026-08-01-deep-cleanup-mvp-aligned-workspace` 已完成所有结构性清理（删除根 `synthia-examples` 段、收紧 workspace deps、README 对齐、tombstone 注释清理、scripts 整理、archive 元数据标记），并以 `make dev` + mvp-smoke PASS（AC-11）作为 E2E 贯通闸门通过
- 该变更的 **Phase 1 baseline** 步骤（tasks.md Layer 1.0-1.7）生成了 4 个 `.txt` 文件作为执行证据：cargo tree 快照、grep tombstone 列表、E2E 基线 PASS 记录、README diff 占位（实际未写入所以是 0 字节）
- 这 4 个文件从一开始就是 **untracked**（`.gitignore` 没列入，但提交时也未 `git add`），属于"工作区临时副产物"
- 前一轮 archive 提案的"完成态"只到 `git status` clean + E2E PASS，没有"清场"步骤 — 这是正常流程缺陷，不是 bug
- 用户的硬约束：**不动 archive**，不重跑 mvp-smoke，不 `cargo clean`；只删除残留 + 跑新扫描 + 落盘清单

## 1. 删除目标清单（4 个 tracked 文件）

```
仓库根 /home/crochee/workspace/synthia/
├── cleanup-baseline-e2e.txt        2 104 B   (Layer 1.0.6 E2E 基线)
├── cleanup-baseline-readme.txt     0 B       (Layer 1.3 README diff 占位，未写入)
├── cleanup-baseline-tombstones.txt 29 215 B  (Layer 1.2 grep 全文)
└── cleanup-baseline-tree.txt       64 701 B  (Layer 1.1 cargo tree 快照)
                                      ───────
                                       96 020 B (~95 KB)
```

**删除前 3 重证据**（已 `git ls-files` + `git log` + grep 验证）：
1. `git ls-files cleanup-baseline-*.txt` → 4 行命中（确认 tracked）
2. `git log --oneline --all -- cleanup-baseline-tree.txt` → `c2096005 chore: remove tombstone comments referencing deleted crates`（确认提交来源：与 archive 提案的 tombstone 清理动作一起进库）
3. `git status` → `nothing to commit, working tree clean`（说明 4 个文件已 commit、无未提交改动）
4. `grep -rn 'cleanup-baseline' . --exclude-dir={target,node_modules,.git,sessions}` → 6 处命中，全部在 `openspec/changes/archive/2026-08-01-deep-cleanup-mvp-aligned-workspace/{design,tasks}.md`（历史归档，**保留**）；本提案自身目录 `openspec/changes/2026-08-01-post-cleanup-residue-and-rescan/` 内的引用不算"依赖"，因为我们只描述删除目标，不依赖它构建/运行

**信息安全性证明**：
- `cleanup-baseline-e2e.txt` 内容（`Phase 1.0 验证 PASS` + wall-clock 秒数）已固化在 archive `tasks.md` 第 1.0.6 行（"记录基线结果到 `cleanup-baseline-e2e.txt`"）
- `cleanup-baseline-tree.txt` 的 `cargo tree -p synthia-server` 输出是 `Cargo.lock` 派生信息 — `Cargo.lock` 本身就是 source of truth，删除 `tree.txt` 不影响 lockfile 一致性（且 commit 进入 git 历史后可永久从 `git show c2096005:cleanup-baseline-tree.txt` 还原）
- `cleanup-baseline-tombstones.txt` 的 grep 列表在 archive `tasks.md` Layer 5.1-5.2 已逐项改写 / 删除完毕（即所有 tombstone 引用已清，列表本身无信息增量）
- `cleanup-baseline-readme.txt` 是 0 字节，无信息

**回滚保证**：4 个文件删除通过 `git commit` 永久记录 deletion，commit 之后任意时刻可通过 `git revert <commit-hash>` 完整恢复所有 4 个文件（也包括 `git checkout <commit-hash>~1 -- cleanup-baseline-*.txt` 局部恢复）。

**结论**：删除零信息损失 + 完整可回滚。

## 2. 扫描方法

只读扫描，不在本提案内修改任何 .rs / Cargo.toml：

```bash
# 1. clippy（已跑过）
cargo +nightly clippy --all-targets --all-features --tests --all
# 期望: exit 0, 0 warning
# 实测: 2m 52s, Finished `dev` profile [unoptimized + debuginfo] target(s)

# 2. fmt --check（已跑过）
cargo +nightly fmt --all -- --check
# 期望: exit 0, 无 diff
# 实测: 无 stdout 输出（即已格式化）

# 3. machete（已跑过）
cargo machete --with-metadata
# 期望: 列出"疑似未用依赖"清单（不修改任何文件）
# 实测: 见第 4 节"扫描结果摘要"

# 4. udeps（不可用，跳过并标注原因）
cargo udeps --all-targets
# 期望: 不可用
# 实测: error: no such command: `udeps`
# 决定: 本提案不安装 cargo-udeps（避免新依赖），仅在 scan-results.md 标注"未执行"
```

扫描结果**全部落盘**到 `scan-results.md`，供后续变更决策（本提案不处理）。

## 3. 分层执行策略

```
Layer 0: 前置验证（不改代码）
├─ 0.1 git status 确认 clean
├─ 0.2 ls 确认 4 个 tracked 文件存在 + 大小匹配
├─ 0.3 git ls-files 确认 4 个文件在 index 中
├─ 0.4 git log --oneline 确认提交来源（c2096005）
└─ 0.5 grep 确认无脚本/CI/Makefile 引用这些文件

Layer 1: 删除残留（git rm 路径）
├─ 1.1 git rm cleanup-baseline-*.txt（同时从 index + working tree 移除，stage deletion；git rm 不支持 -v，所以会用 4 行 `rm '...'` 默认输出）
├─ 1.2 ls 二次确认 0 匹配
├─ 1.3 git status 二次确认 4 个 deleted 已 stage
├─ 1.4 grep 二次确认 archive 内的 6 处历史记录仍保留
└─ 1.5 git commit -m "chore(repo): remove cleanup-baseline residue from archive proposal" 落库

Layer 2: 轻量新扫描（只读，不改码）
├─ 2.1 cargo +nightly clippy --all-targets --all-features --tests --all
├─ 2.2 cargo +nightly fmt --all -- --check
├─ 2.3 cargo +nightly fmt --all（同步格式，防止 --check 通过但需 sync 步）
├─ 2.4 cargo machete --with-metadata
├─ 2.5 cargo udeps --all-targets（预期失败，记录原因）
└─ 2.6 把 2.1-2.5 输出写进 scan-results.md

Layer 3: 分批测试回归（按 AGENTS.md 禁止 --workspace）
├─ 3.1 cargo test -p synthia-core
├─ 3.2 cargo test -p synthia-telemetry
├─ 3.3 cargo test -p synthia-provider
├─ 3.4 cargo test -p synthia-hook
├─ 3.5 cargo test -p synthia-context
├─ 3.6 cargo test -p synthia-tool
├─ 3.7 cargo test -p synthia-command
├─ 3.8 cargo test -p synthia-skill
├─ 3.9 cargo test -p synthia-session
├─ 3.10 cargo test -p synthia-agent
├─ 3.11 cargo test -p synthia-cli
├─ 3.12 cargo test -p synthia-server
├─ 3.13 cargo test -p synthia-job
├─ 3.14 cargo test -p synthia-cache-mark
├─ 3.15 cargo test -p synthia-protocol
└─ 3.16 cargo test -p synthia-a2a

Layer 4: 终验
├─ 4.1 git status 仍 clean
├─ 4.2 git status --porcelain 全空
├─ 4.3 du -sh /home/crochee/workspace/synthia/cleanup-baseline-*.txt 2>&1 期望 "No such file"
└─ 4.4 提交三件套 + scan-results.md 到新提案目录
```

每步独立可验证，单步失败不污染后续步骤。

## 4. 扫描结果摘要（pre-flight 2026-08-01 14:xx UTC+8）

### 4.1 `cargo +nightly clippy --all-targets --all-features --tests --all`
- 状态：**0 warning, exit 0**
- 耗时：2m 52s
- 涵盖：16 个 workspace member + test-support + 所有 features
- 结论：clippy 干净，**本提案无 clippy 回归风险**

### 4.2 `cargo +nightly fmt --all -- --check`
- 状态：**clean, exit 0**
- 结论：格式已对齐

### 4.3 `cargo machete --with-metadata`
- 状态：报告若干"疑似未用依赖"（按 crate 列出）
- 完整输出落盘到 `scan-results.md`
- **本提案不处理这些发现** — 需要逐项 grep 源码确认是否真未用（`#[cfg(test)]` / `dev-dependencies` / 间接引用都可能让 machete 误报）
- 候选清单（待后续变更核查）：
  - 根 `[workspace.dependencies]`：`url`, `uuid` 等（machete 报在最末尾，需对照 grep）
  - `crates/synthia-core/Cargo.toml`：`tokio-util`
  - `crates/synthia-protocol/Cargo.toml`：`chrono`, `pretty_assertions`
  - `crates/synthia-server/Cargo.toml`：`a2a-pb`, `base64`, `serial_test`, `synthia-protocol`, `thiserror`, `tokio-tungstenite`
  - `crates/synthia-telemetry/Cargo.toml`：`thiserror`
  - `crates/synthia-provider/Cargo.toml`：`anyhow`, `async-stream`, `tempfile`, `uuid`
  - `crates/synthia-command/Cargo.toml`：`thiserror`
  - `test-support/Cargo.toml`：`anyhow`, `async-stream`, `chrono`, `futures`, `serde`, `synthia-skill`
- **警示**：machete 在以下场景会误报：
  - `dev-dependencies` 实际被 `#[cfg(test)]` 引用
  - 通过 `pub use` 重导出
  - `build.rs` / `tests/` / `examples/` 引用
  - feature-flagged 依赖
- **本提案决策**：仅记录清单，不修改

### 4.4 `cargo udeps`
- 状态：**未执行**（`error: no such command: udeps`）
- 原因：本机 cargo 未安装 `cargo-udeps` 子命令（需 `cargo install cargo-udeps --locked`）；本提案避免引入新安装步骤
- 替代：machete 已覆盖大部分"未用依赖"场景（精度不同：machete 偏静态，udeps 偏编译图）
- 后续：如需 udeps 精确结果，开新变更安装 + 跑

## 5. 安全网机制

1. **删除前 3 重证据**：untracked 确认 + git status clean + grep 0 引用 → 不会误删
2. **删除后 4 重验证**：ls 0 匹配 + git status clean + grep archive 仍保留 + 4 个文件确实消失
3. **不重跑 E2E**：用户明确未要求；前一轮 archive AC-11 已 PASS
4. **不触发 `cargo clean`**：15 GB `target/` 留给用户按需
5. **clippy / fmt / test 三层回归**：分步执行，任一失败立刻停（不进入下一步）
6. **不动 archive / inbox / CHANGELOG.md / .omo / .trae / vendor/a2a-pb / config.yaml / clippy.toml / deny.toml / rust-toolchain.toml / .gitignore / rustfmt.toml**（用户硬约束）
7. **不重命名/合并 workspace member crate**（用户硬约束）
8. **不主动 push / PR**（按 AGENTS.md 规范）

## 6. 已知风险与缓解

| 风险 | 缓解 |
|---|---|
| 误删被 `git stash` 暂存的同名文件 | `git status` 已报 clean，无 stash；删除前再次 `git stash list` 确认（tasks 1.0） |
| 误删被 IDE/编辑器打开的同名文件 | 4 个 .txt 是历史输出，IDE 不会主动打开；删除前 `lsof \| grep cleanup-baseline` 确认（tasks 1.0） |
| machete 报告的"未用依赖"实际被 `#[cfg(test)]` 引用而误删 | 本提案**不处理** machete 报告项，只落盘到 scan-results.md，由后续变更决策 |
| `cargo udeps` 缺失导致本提案扫描精度不足 | 已在 design 4.4 标注原因 + 替代方案；本提案核心价值是删除残留，扫描是副产物 |
| 删除后某个不为人知的脚本路径依赖这些 .txt | `grep -rn 'cleanup-baseline' . --exclude-dir={target,node_modules,.git,sessions}` 已 0 命中（除 archive 历史），证据已固化 |
| 未来某天需要"恢复"某个 baseline 文件 | archive `tasks.md` Layer 1.0-1.2 已记录"如何重新生成"（`cargo tree -p synthia-server > cleanup-baseline-tree.txt` 等），可一行命令恢复 |

## 7. Non-Goals（明确不做）

- **不重跑 mvp-smoke E2E** — 用户明确未要求
- **不处理 machete 报告的"疑似未用依赖"** — 仅落盘到 `scan-results.md`，由后续变更决策
- **不安装 `cargo-udeps`** — 避免引入新依赖；machete 已基本覆盖
- **不触发 `cargo clean`** — 15 GB `target/` 留给用户按需
- **不重命名/合并 workspace member crate** — 用户硬约束
- **不动 archive / inbox / CHANGELOG.md / .omo / .trae / vendor/a2a-pb / config.yaml / clippy.toml / deny.toml / rust-toolchain.toml / .gitignore / rustfmt.toml** — 用户硬约束
- **不主动 git push / 不创建 PR** — 按 AGENTS.md 规范
- **不修改任何 .rs / .toml / .md（除本提案的 proposal/design/tasks/scan-results）** — 副产物清单只读

## 8. 与前一轮 archive 提案的关系

| 维度 | 前一轮 archive | 本提案 |
|---|---|---|
| 目的 | MVP 编译图对齐 + 文档同步 + tombstone 清理 | 前一轮残留文件清场 + 轻量新扫描 |
| 范围 | 改 .rs / Cargo.toml / README / scripts | 仅删除 4 个 untracked .txt + 落盘 scan-results.md |
| 深度 | 激进（15 个 Layer 改动） | 极浅（1 个文件系统动作） |
| 验证门禁 | E2E AC-11 mvp-smoke | clippy 0 + fmt 0 + 16 个分批 test 绿 |
| E2E | 必须重跑 | 不重跑（前一轮已验证） |
| 持续时间 | 多日 | 数分钟 |
