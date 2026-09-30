## 1. 前置验证（不改代码）

- [ ] **1.1** 确认 `git status` 报 clean（基线状态）：
  ```bash
  git status
  ```
  期望：`nothing to commit, working tree clean`

- [ ] **1.2** 确认 4 个 tracked 文件存在 + 大小匹配（与 proposal 表格一致）：
  ```bash
  stat -c '%n %s bytes' cleanup-baseline-*.txt
  ```
  期望：
  ```
  cleanup-baseline-e2e.txt 2104 bytes
  cleanup-baseline-readme.txt 0 bytes
  cleanup-baseline-tombstones.txt 29215 bytes
  cleanup-baseline-tree.txt 64701 bytes
  ```

- [ ] **1.3** 确认 4 个文件都在 git index 中（tracked，不是 untracked）：
  ```bash
  git ls-files cleanup-baseline-*.txt | wc -l
  ```
  期望：`4`

- [ ] **1.4** 确认提交来源（archive 提案同一批 commit `c2096005`）：
  ```bash
  git log --oneline --all -- cleanup-baseline-tree.txt | head -3
  ```
  期望：包含 `c2096005 chore: remove tombstone comments referencing deleted crates`

- [ ] **1.5** 确认无任何脚本 / CI / Makefile 引用这些文件名（除 archive 历史 + 本提案目录）：
  ```bash
  grep -rn 'cleanup-baseline' . --exclude-dir=target --exclude-dir=node_modules --exclude-dir=.git --exclude-dir=sessions | wc -l
  ```
  期望：`48`（6 处 archive 历史 + 42 处本提案自身目录引用 = 48；本提案目录的引用是描述删除目标，不算"依赖"）

- [ ] **1.6** 确认无 git stash 暂存同名文件：
  ```bash
  git stash list | grep cleanup-baseline || echo "no stash matches"
  ```
  期望：输出 `no stash matches`

- [ ] **1.7** 确认无进程打开这些文件（避免删除时 busy）：
  ```bash
  lsof 2>/dev/null | grep cleanup-baseline || echo "no process holds these files"
  ```
  期望：输出 `no process holds these files`

## 2. 删除 4 个 tracked 残留文件（git rm 路径）

- [ ] **2.1** 一次性用 `git rm` 删除 4 个文件（同时从 index + working tree 移除，stage deletion）：
  ```bash
  git rm cleanup-baseline-e2e.txt cleanup-baseline-readme.txt cleanup-baseline-tombstones.txt cleanup-baseline-tree.txt
  ```
  期望：4 行 `rm '...'` 输出

- [ ] **2.2** 确认 `ls` 找不到任何 `cleanup-baseline-*.txt`（working tree 已清空）：
  ```bash
  ls /home/crochee/workspace/synthia/cleanup-baseline-*.txt 2>/dev/null | wc -l
  ```
  期望：`0`

- [ ] **2.3** 确认 `git status` 显示 4 个 deletion 已 stage：
  ```bash
  git status
  ```
  期望：包含 `Changes to be committed:` + `deleted:    cleanup-baseline-e2e.txt` 等 4 行 deleted

- [ ] **2.4** 确认 `git status --porcelain` 非空（4 个 `D` 状态）：
  ```bash
  git status --porcelain | wc -l
  ```
  期望：`4`

- [ ] **2.5** 确认 archive 内的 6 处历史记录仍保留（**不应被波及**）：
  ```bash
  grep -rn 'cleanup-baseline' openspec/changes/archive/2026-08-01-deep-cleanup-mvp-aligned-workspace/ | wc -l
  ```
  期望：`6`（design.md 1 处 + tasks.md 5 处）

- [ ] **2.6** 确认本提案目录内 42 处引用仍保留（这些是描述性引用，不是依赖）：
  ```bash
  grep -rn 'cleanup-baseline' openspec/changes/2026-08-01-post-cleanup-residue-and-rescan/ | wc -l
  ```
  期望：`42`（design.md / tasks.md / proposal.md 内的描述性引用，**保留**）

- [ ] **2.7** 提交删除动作（按 AGENTS.md "atomic commits"）：
  ```bash
  git add openspec/changes/2026-08-01-post-cleanup-residue-and-rescan/  # 本提案三件套
  git commit -m "chore(repo): remove cleanup-baseline residue from archive proposal

  - Delete 4 untracked-by-intent baseline files captured in c2096005:
    cleanup-baseline-{e2e,readme,tombstones,tree}.txt
  - All content already preserved in:
    openspec/changes/archive/2026-08-01-deep-cleanup-mvp-aligned-workspace/
  - Resolves post-cleanup residue from MVP-aligned-workspace proposal"
  ```
  期望：`[master <hash>] chore(repo): remove cleanup-baseline residue...`，1 个 commit，4 files changed, 1497 deletions(-)

## 3. 轻量新扫描（只读，不改码）

- [ ] **3.1** 跑 clippy 全量：
  ```bash
  cargo +nightly clippy --all-targets --all-features --tests --all 2>&1 | tee /tmp/scan-clippy.log
  ```
  期望：最后一行 `Finished ... target(s) in <Xm Ys>`，无 `warning:` / `error:` 出现
  实测（pre-flight 2026-08-01）：2m 52s，0 warning，exit 0

- [ ] **3.2** 跑 fmt 检查：
  ```bash
  cargo +nightly fmt --all -- --check 2>&1 | tee /tmp/scan-fmt.log
  ```
  期望：exit 0，stdout 空（即已格式化）
  实测（pre-flight 2026-08-01）：clean，exit 0

- [ ] **3.3** 同步 fmt（防止 --check 通过但需要 sync 步）：
  ```bash
  cargo +nightly fmt --all 2>&1 | tee /tmp/scan-fmt-sync.log
  ```
  期望：exit 0，无 diff（即已对齐）

- [ ] **3.4** 跑 machete：
  ```bash
  cargo machete --with-metadata 2>&1 | tee /tmp/scan-machete.log
  ```
  期望：列出若干"疑似未用依赖"清单（按 crate 分组），**不修改任何文件**
  实测（pre-flight 2026-08-01）：见 design.md 第 4.3 节

- [ ] **3.5** 尝试 udeps（预期失败，记录原因）：
  ```bash
  cargo udeps --all-targets 2>&1 | tee /tmp/scan-udeps.log
  ```
  期望：`error: no such command: udeps`
  行动：把失败原因 + "本提案未安装 cargo-udeps" 写进 `scan-results.md` 第 2 节

- [ ] **3.6** 把 3.1-3.5 的输出汇总到 `scan-results.md`：
  ```bash
  cat > openspec/changes/2026-08-01-post-cleanup-residue-and-rescan/scan-results.md <<'EOF'
  # Lightweight Rescan Results — 2026-08-01

  ## 1. cargo +nightly clippy
  - exit 0, 0 warning
  - 耗时: <Xm Ys>（填写实际值）
  - 命令: `cargo +nightly clippy --all-targets --all-features --tests --all`
  - 关键输出最后一行: `Finished ...`

  ## 2. cargo +nightly fmt --check
  - exit 0, 0 diff
  - 命令: `cargo +nightly fmt --all -- --check`

  ## 3. cargo +nightly fmt
  - exit 0, 0 diff（同步步）
  - 命令: `cargo +nightly fmt --all`

  ## 4. cargo machete
  - 报告若干"疑似未用依赖"（不修改文件）
  - 候选清单（按 crate）：
    - <粘贴 /tmp/scan-machete.log 的内容>
  - **警示**：machete 在以下场景会误报：
    - dev-dependencies + #[cfg(test)] 引用
    - pub use 重导出
    - build.rs / tests / examples 引用
    - feature-flagged 依赖
  - **本提案决策**：仅记录清单，不修改

  ## 5. cargo udeps
  - 状态: **未执行**
  - 错误: `error: no such command: 'udeps'`
  - 原因: 本机 cargo 未安装 cargo-udeps（需 `cargo install cargo-udeps --locked`）；本提案避免引入新安装步骤
  - 替代: machete 已覆盖大部分"未用依赖"场景（精度不同）
  - 后续: 如需 udeps 精确结果，开新变更安装 + 跑
  EOF
  ```
  期望：scan-results.md 创建成功，5 节齐全

- [ ] **3.7** 确认 `git status` 仅含本提案目录作为 untracked：
  ```bash
  git status --porcelain
  ```
  期望：仅显示 `?? openspec/changes/2026-08-01-post-cleanup-residue-and-rescan/`（untracked 新提案目录），**无其他 untracked**

## 4. 分批测试回归（按 AGENTS.md 禁止 `cargo test --workspace`）

每个 crate 独立跑，任一失败立即停：

- [ ] **4.1** `cargo test -p synthia-core`
- [ ] **4.2** `cargo test -p synthia-telemetry`
- [ ] **4.3** `cargo test -p synthia-provider`
- [ ] **4.4** `cargo test -p synthia-hook`
- [ ] **4.5** `cargo test -p synthia-context`
- [ ] **4.6** `cargo test -p synthia-tool`
- [ ] **4.7** `cargo test -p synthia-command`
- [ ] **4.8** `cargo test -p synthia-skill`
- [ ] **4.9** `cargo test -p synthia-session`
- [ ] **4.10** `cargo test -p synthia-agent`
- [ ] **4.11** `cargo test -p synthia-cli`
- [ ] **4.12** `cargo test -p synthia-server`
- [ ] **4.13** `cargo test -p synthia-job`
- [ ] **4.14** `cargo test -p synthia-cache-mark`
- [ ] **4.15** `cargo test -p synthia-protocol`
- [ ] **4.16** `cargo test -p synthia-a2a`

每个步骤期望：`test result: ok. <N> passed; 0 failed`（或 `0 passed` for crate without tests），exit 0
**如任一 FAIL**：立即停止后续步骤，定位失败 crate 修复后从 4.x 失败步骤重跑

## 5. 终验（AC-1 / AC-2 / AC-3 / AC-3b / AC-9 / AC-10 / AC-11 全通过）

- [ ] **5.1** `git status` 最终 clean（含本提案目录作为唯一 untracked）：
  ```bash
  git status
  ```
  期望：`Untracked files: ... openspec/changes/2026-08-01-post-cleanup-residue-and-rescan/`（仅此 1 项），其他 `nothing to commit`

- [ ] **5.2** 确认 4 个 .txt 确实从磁盘消失：
  ```bash
  test ! -e cleanup-baseline-e2e.txt && test ! -e cleanup-baseline-readme.txt && test ! -e cleanup-baseline-tombstones.txt && test ! -e cleanup-baseline-tree.txt && echo "all 4 files gone"
  ```
  期望：`all 4 files gone`

- [ ] **5.3** 确认 4 个 .txt 在 archive 历史中**仍**有引用（**不应**误删 archive 内容）：
  ```bash
  grep -rn 'cleanup-baseline' openspec/changes/archive/2026-08-01-deep-cleanup-mvp-aligned-workspace/ | wc -l
  ```
  期望：`6`

- [ ] **5.4** 确认新提案三件套齐备：
  ```bash
  ls -la openspec/changes/2026-08-01-post-cleanup-residue-and-rescan/
  ```
  期望：
  ```
  proposal.md
  design.md
  tasks.md
  scan-results.md
  ```

- [ ] **5.5** 确认 `target/` 未被本提案清空（用户硬约束）：
  ```bash
  du -sh /home/crochee/workspace/synthia/target
  ```
  期望：~15G（与 1.0 步骤基线一致）

- [ ] **5.6** 确认 archive / inbox / CHANGELOG.md / .omo / .trae / vendor/a2a-pb / config.yaml / clippy.toml / deny.toml / rust-toolchain.toml / .gitignore / rustfmt.toml **均无修改**：
  ```bash
  git status --porcelain openspec/changes/archive/ openspec/changes/_inbox/ CHANGELOG.md .omo/ .trae/ vendor/a2a-pb/ config.yaml clippy.toml deny.toml rust-toolchain.toml .gitignore rustfmt.toml
  ```
  期望：空输出

- [ ] **5.7** 确认删除 commit 已落地（`git log` 显示新 commit）：
  ```bash
  git log --oneline -3
  ```
  期望：最顶 commit message 包含 `chore(repo): remove cleanup-baseline residue from archive proposal`

- [ ] **5.8** 确认无任何 push / PR：
  ```bash
  git log --oneline @{u}..HEAD 2>/dev/null | head -5 || echo "no upstream tracking"
  ```
  期望：空输出（无新 commit 推送到远程）

## 6. 不做的事（明确清单）

- ❌ **不重跑 mvp-smoke E2E**（前一轮 archive AC-11 已 PASS，用户未要求复跑）
- ❌ **不触发 `cargo clean`**（15 GB `target/` 留给用户）
- ❌ **不处理 machete 报告的"疑似未用依赖"**（仅落盘到 scan-results.md）
- ❌ **不安装 `cargo-udeps`**（避免引入新依赖）
- ❌ **不动 archive / inbox / CHANGELOG.md / .omo / .trae / vendor/a2a-pb / config.yaml / clippy.toml / deny.toml / rust-toolchain.toml / .gitignore / rustfmt.toml**
- ❌ **不重命名/合并 workspace member crate**
- ❌ **不主动 git push / 不创建 PR**
- ❌ **不修改任何 .rs / .toml（除本提案目录内的 proposal/design/tasks/scan-results）**
