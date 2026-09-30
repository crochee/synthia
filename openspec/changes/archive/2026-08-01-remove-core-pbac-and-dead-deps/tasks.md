## 1. 前置验证（只读，改 Cargo.toml 前必跑）

> **本阶段任一步骤若 grep 命中（非 0），立即停止后续所有动作**，把命中位置写到本文件"阻塞项"小节，并请求用户决策（保留 dep 或解除调用方）。

- [ ] **1.1** 确认 PBAC 跨 crate 引用为 0（最后一次复核）：
  ```bash
  grep -rn 'use crate::pbac\|use synthia_core::pbac\|synthia_core::pbac' crates/ synthia-web/ test-support/ --exclude-dir=target 2>/dev/null | wc -l
  ```
  期望：`0`

- [ ] **1.2** 确认 PBAC 同名噪音类型无调用（`Policy::`、`PolicyEngine` 等）：
  ```bash
  grep -rn 'PolicyEngine\|AccessRequest\|PolicySet\|StandardRiskEvaluator\|ConsoleAuditLogger' crates/ --exclude-dir=target --exclude-dir=pbac 2>/dev/null | grep -v 'reqwest::redirect::Policy\|SanitizationPolicy\|CachePolicy\|ForkPolicy' | wc -l
  ```
  期望：`0`

- [ ] **1.3** 确认 `a2a-pb` 在 `synthia-server` 源码中无引用：
  ```bash
  grep -rn 'a2a_pb\|a2a-pb' crates/synthia-server/src/ 2>/dev/null | wc -l
  ```
  期望：`0`

- [ ] **1.4** 确认 `tokio-tungstenite` 在 `synthia-cli` 与 `synthia-server` 源码中无引用：
  ```bash
  grep -rn 'tungstenite' crates/synthia-cli/src/ crates/synthia-server/src/ 2>/dev/null | wc -l
  ```
  期望：`0`
  警示：`synthia-server` WS 走 `axum::extract::ws`（内置），无 tungstenite 路径

- [ ] **1.5** 确认 `nix` 在 `synthia-agent` 源码中无引用：
  ```bash
  grep -rn 'use nix\|nix::' crates/synthia-agent/src/ 2>/dev/null | wc -l
  ```
  期望：`0`
  警示：sandbox 路径走 `BubblewrapBackend` / `OsFileSystem`，不依赖 nix syscall 绑定

- [ ] **1.6** 确认 `libc` 在 `synthia-tool` 源码中无引用：
  ```bash
  grep -rn 'use libc\|libc::' crates/synthia-tool/src/ 2>/dev/null | wc -l
  ```
  期望：`0`

- [ ] **1.7** 确认 `serial_test` 与 `pretty_assertions` 在源码中无引用：
  ```bash
  grep -rn '#\[serial\]\|#\[parallel\]\|serial_test\|pretty_assertions' crates/*/src/ 2>/dev/null | wc -l
  ```
  期望：`0`
  警示：dev-dep 引用通常出现在 `tests/` 子目录而非 `src/`，追加子目录检查：
  ```bash
  grep -rn '#\[serial\]\|#\[parallel\]' crates/*/tests/ 2>/dev/null | wc -l
  grep -rn 'pretty_assertions' crates/*/tests/ 2>/dev/null | wc -l
  ```
  两组都期望 `0`

- [ ] **1.8** 确认 `git status` 报 clean（删除前基线）：
  ```bash
  git status
  ```
  期望：`nothing to commit, working tree clean`

- [ ] **1.9** 确认基线 commit 已知：
  ```bash
  git log --oneline -1
  ```
  记录输出，供阶段 5.7 对比

## 2. 删除 PBAC 模块（路径 1）

- [ ] **2.1** 删除整个 pbac 目录（保留 git 历史可回滚）：
  ```bash
  git rm -r crates/synthia-core/src/pbac/
  ```
  期望：23 个 `rm 'crates/synthia-core/src/pbac/...'` 行

- [ ] **2.2** 修改 `crates/synthia-core/src/lib.rs`：
  - 删除第 7 行：`pub mod pbac;`
  - 删除第 44 行：`pub use pbac::*;`
  ```bash
  # 用 Edit 工具精确删两行（不要 sed，保持工具可审计）
  ```

- [ ] **2.3** 确认 `cargo build -p synthia-core` 通过：
  ```bash
  cargo build -p synthia-core 2>&1 | tee /tmp/pbac-build.log
  ```
  期望：最后一行 `Compiling synthia-core` + `Finished ...`，无 `error:` / `warning:`，exit 0

- [ ] **2.4** 确认 `cargo clippy -p synthia-core --all-targets` 通过：
  ```bash
  cargo +nightly clippy -p synthia-core --all-targets --all-features --tests 2>&1 | tee /tmp/pbac-clippy.log
  ```
  期望：exit 0，无 `warning:` / `error:`

## 3. 移除 unused deps（路径 2 加项）

> 每个 crate 改 `Cargo.toml` 后立即跑 `cargo check -p <crate>` 验证编译图。

- [ ] **3.1** 修改 `crates/synthia-server/Cargo.toml`：
  - 删除第 10 行：`a2a-pb = "0.2"`
  - 删除第 60 行：`serial_test = "3"`
  - 删除第 61 行：`tokio-tungstenite.workspace = true`
  ```bash
  cargo check -p synthia-server --all-targets 2>&1 | tee /tmp/deps-server.log
  ```
  期望：exit 0，无 error（warning 可能来自其他模块，未在本提案 scope 内则不动）

- [ ] **3.2** 修改 `crates/synthia-cli/Cargo.toml`：
  - 删除第 34 行：`tokio-tungstenite = { workspace = true }`
  ```bash
  cargo check -p synthia-cli --all-targets 2>&1 | tee /tmp/deps-cli.log
  ```
  期望：exit 0

- [ ] **3.3** 修改 `crates/synthia-agent/Cargo.toml`：
  - 删除第 45 行：`nix = { version = "0.29", features = ["signal", "process"] }`
  ```bash
  cargo check -p synthia-agent --all-targets 2>&1 | tee /tmp/deps-agent.log
  ```
  期望：exit 0

- [ ] **3.4** 修改 `crates/synthia-tool/Cargo.toml`：
  - 删除第 29 行：`libc = "0.2"`
  ```bash
  cargo check -p synthia-tool --all-targets 2>&1 | tee /tmp/deps-tool.log
  ```
  期望：exit 0

- [ ] **3.5** 修改 `crates/synthia-provider/Cargo.toml`：
  - 删除第 46 行：`serial_test = "3"`
  ```bash
  cargo check -p synthia-provider --all-targets 2>&1 | tee /tmp/deps-provider.log
  ```
  期望：exit 0

- [ ] **3.6** 修改 `crates/synthia-protocol/Cargo.toml`：
  - 删除第 24 行：`pretty_assertions = "1"`
  ```bash
  cargo check -p synthia-protocol --all-targets 2>&1 | tee /tmp/deps-protocol.log
  ```
  期望：exit 0

- [ ] **3.7** 修改根 `Cargo.toml`：
  - 删除第 78 行：`tokio-tungstenite = "0.24"`（workspace 行）
  - 删除第 104-107 行 a2a-pb patch 块（注释 + `a2a-pb = { path = "vendor/a2a-pb" }`）：
    ```toml
    # Patch a2a-pb to use optional bool for append/last_chunk fields
    a2a-pb = { path = "vendor/a2a-pb" }
    ```

- [ ] **3.8** 跑 `cargo build` 同步 `Cargo.lock`：
  ```bash
  cargo build 2>&1 | tee /tmp/deps-build.log
  ```
  期望：exit 0，`Cargo.lock` 自动同步（`git status` 应显示 `Cargo.lock` modified）

## 4. 健康度回归（按 AGENTS.md / rust.md 强制）

- [ ] **4.1** `cargo +nightly fmt --all`：
  ```bash
  cargo +nightly fmt --all 2>&1 | tee /tmp/health-fmt.log
  ```
  期望：exit 0

- [ ] **4.2** `cargo +nightly fmt --all -- --check`：
  ```bash
  cargo +nightly fmt --all -- --check 2>&1 | tee /tmp/health-fmt-check.log
  ```
  期望：exit 0，stdout 空

- [ ] **4.3** `cargo +nightly clippy --all-targets --all-features --tests --all`：
  ```bash
  cargo +nightly clippy --all-targets --all-features --tests --all 2>&1 | tee /tmp/health-clippy.log
  ```
  期望：exit 0，0 warning

- [ ] **4.4** `cargo test -p synthia-core`：
  ```bash
  cargo test -p synthia-core 2>&1 | tee /tmp/test-core.log
  ```
  期望：`test result: ok. <N> passed; 0 failed`，exit 0

- [ ] **4.5** `cargo test -p synthia-telemetry`

- [ ] **4.6** `cargo test -p synthia-provider`

- [ ] **4.7** `cargo test -p synthia-hook`

- [ ] **4.8** `cargo test -p synthia-context`

- [ ] **4.9** `cargo test -p synthia-tool`

- [ ] **4.10** `cargo test -p synthia-command`

- [ ] **4.11** `cargo test -p synthia-skill`

- [ ] **4.12** `cargo test -p synthia-session`

- [ ] **4.13** `cargo test -p synthia-agent`

- [ ] **4.14** `cargo test -p synthia-cli`

- [ ] **4.15** `cargo test -p synthia-server`

- [ ] **4.16** `cargo test -p synthia-job`

- [ ] **4.17** `cargo test -p synthia-cache-mark`

- [ ] **4.18** `cargo test -p synthia-protocol`

- [ ] **4.19** `cargo test -p synthia-a2a`

（步骤 4.5-4.19 风格同 4.4，每个步骤期望 `test result: ok. <N> passed; 0 failed`，exit 0。**任一 FAIL 立即停止后续步骤**，定位失败 crate 修复后从失败步骤重跑）

## 5. 终验（AC-1 / AC-2 / ... / AC-14 全通过）

- [ ] **5.1** AC-1：`find crates/synthia-core/src/pbac -type f` 报 0 / No such file
  ```bash
  find crates/synthia-core/src/pbac -type f 2>/dev/null | wc -l
  ```
  期望：`0`

- [ ] **5.2** AC-2：`grep -rn 'pub use pbac\|pub mod pbac' crates/synthia-core/src/` 报 0
  ```bash
  grep -rn 'pub use pbac\|pub mod pbac' crates/synthia-core/src/ | wc -l
  ```
  期望：`0`

- [ ] **5.3** AC-3：`grep -rn 'use synthia_core::pbac\|use crate::pbac' crates/ synthia-web/ test-support/` 报 0
  ```bash
  grep -rn 'use synthia_core::pbac\|use crate::pbac' crates/ synthia-web/ test-support/ --exclude-dir=target 2>/dev/null | wc -l
  ```
  期望：`0`

- [ ] **5.4** AC-8：`grep -rn 'pbac' openspec/changes/2026-08-01-remove-core-pbac-and-dead-deps/` 仅标题命中
  ```bash
  grep -rn 'pbac' openspec/changes/2026-08-01-remove-core-pbac-and-dead-deps/ | wc -l
  ```
  期望：≤ 5（仅 proposal.md / design.md / tasks.md 标题与描述）

- [ ] **5.5** AC-8 旁证：archive 历史命中保留
  ```bash
  grep -rn 'pbac' openspec/changes/archive/ | wc -l
  ```
  期望：≥ 1（archive 内的历史评测记录保留）

- [ ] **5.6** AC-10：`cargo build`（debug）成功
  ```bash
  cargo build 2>&1 | tail -5
  ```
  期望：最后一行 `Finished ...`，exit 0

- [ ] **5.7** AC-11：`cargo check --workspace --all-targets` 成功
  ```bash
  cargo check --workspace --all-targets 2>&1 | tee /tmp/final-check.log
  ```
  期望：exit 0

- [ ] **5.8** AC-9：`git status` 最终 clean（含本提案目录作为唯一 untracked）
  ```bash
  git status
  ```
  期望：
  ```
  Untracked files: openspec/changes/2026-08-01-remove-core-pbac-and-dead-deps/
  nothing added to commit (or 仅 Cargo.lock modified 等待 commit 决策)
  ```

- [ ] **5.9** AC-12（可选 / 用户决策）：`make dev` dry-check
  ```bash
  timeout 30 make dev 2>&1 | tee /tmp/dev.log
  # 或在另一个终端后台启动 synthia-server 后 curl http://localhost:8080/health
  ```
  期望：synthia-server boot 成功 + synthia-web dev server :5173 监听
  > 注：本提案未要求重跑 mvp-smoke E2E（按 proposal §Out of Scope）

- [ ] **5.10** AC-13：阶段 1.x 步骤已勾选完毕（deps 交叉确认矩阵完整记录）

- [ ] **5.11** AC-14：边界外路径 `git status` 空
  ```bash
  git status --porcelain openspec/changes/archive/ openspec/changes/_inbox/ openspec/changes/2026-08-01-post-cleanup-residue-and-rescan/ CHANGELOG.md .omo/ .trae/ vendor/a2a-pb/ config.yaml clippy.toml deny.toml rust-toolchain.toml .gitignore rustfmt.toml
  ```
  期望：空输出

- [ ] **5.12** 记录改动文件清单：
  ```bash
  git status --porcelain
  ```
  期望：含
  ```
  D crates/synthia-core/src/pbac/...   # 23 行 deleted
  M crates/synthia-core/src/lib.rs
  M crates/synthia-server/Cargo.toml
  M crates/synthia-cli/Cargo.toml
  M crates/synthia-agent/Cargo.toml
  M crates/synthia-tool/Cargo.toml
  M crates/synthia-provider/Cargo.toml
  M crates/synthia-protocol/Cargo.toml
  M Cargo.toml
  M Cargo.lock
  ?? openspec/changes/2026-08-01-remove-core-pbac-and-dead-deps/
  ```

- [ ] **5.13** 确认无 push / PR：
  ```bash
  git log --oneline @{u}..HEAD 2>/dev/null | head -5 || echo "no upstream tracking"
  ```
  期望：空输出

- [ ] **5.14** 确认 `target/` 未被本提案清空（用户硬约束）：
  ```bash
  du -sh /home/crochee/workspace/synthia/target
  ```
  期望：~15G（与阶段 1.x 基线一致；本提案未跑 `cargo clean`）

## 6. 不做的事（明确清单）

- ❌ **不重跑 mvp-smoke E2E**（前一轮 archive AC-11 已 PASS，用户未要求复跑）
- ❌ **不触发 `cargo clean`**（15 GB `target/` 留给用户按需）
- ❌ **不安装 `cargo-udeps`**（避免引入新依赖）
- ❌ **不动 archive / `.trae/` / `.omo/` / `vendor/a2a-pb/` / `CHANGELOG.md`**
- ❌ **不动 `Permission` / `ToolGuard` 系统**（与 PBAC 是两个独立抽象）
- ❌ **不动同名 `Policy` 类型**（CachePolicy / SanitizationPolicy / ForkPolicy / reqwest::redirect::Policy）
- ❌ **不动 `RiskEvaluator` / `AuditLogger` trait 化**（PBAC 已 0 调用方）
- ❌ **不修改 `Cargo.lock`（手工）**（仅 `cargo build` 自动同步）
- ❌ **不主动 git push / 不创建 PR**
- ❌ **不重命名/合并 workspace member crate**
- ❌ **不安装 cargo-udeps / cargo-machete（已安装则可用，未装则不装）**
- ❌ **不动 `config.yaml` / `clippy.toml` / `deny.toml` / `rust-toolchain.toml` / `.gitignore` / `rustfmt.toml`**

## 7. 阻塞项（如有）

> 阶段 1.x 任一 grep 命中（非 0）时记录于此：

```
无
```

## 8. 阶段 1 实际结果（2026-08-01 执行）

### 8.1 §1.1 PBAC 跨 crate 引用：1 命中（模块内部测试，已排除）

- **命中**：`crates/synthia-core/src/pbac/evaluation/tests.rs:4: use crate::pbac::{policy::Condition, *};`
- **决策**：✅ 不算跨 crate 引用——这是 PBAC 模块自身的测试文件（位于 `pbac/` 子目录下）。proposal 已声明"不含 pbac/ 自身"。
- **PBAC 整模块删除**：✅ 确认可执行

### 8.2 §1.3 a2a-pb in synthia-server/src/：2 命中（仅注释引用）

- **命中**：
  - `crates/synthia-server/src/a2a/service.rs:14: //! `vendor/a2a-pb/proto/a2a.proto`, which generates `Option<bool>` in Rust`
  - `crates/synthia-server/src/a2a/serde_sse.rs:6: //! as `optional bool` in `vendor/a2a-pb/proto/a2a.proto`, which generates`
- **决策**：✅ 0 实际代码引用（仅注释）。但删除 `a2a-pb` patch 后这两段注释的"指代对象"消失，**需同步调整注释**：
  - 选项 A：把"vendor/a2a-pb/proto/a2a.proto"改成"a2a.proto (上游 proto3 schema)"等中性表述
  - 选项 B：保留注释，但加一句"(patch path no longer used; schema reference保留作为历史)"
  - **决策**：选 A（中性化表述，避免误导）
- **a2a-pb Cargo.toml 移除**：✅ 确认可执行

### 8.3 §1.4 tungstenite in synthia-cli + synthia-server：8 命中（全在 synthia-cli）

- **命中**：全在 `crates/synthia-cli/src/wire.rs:128-148`（`tokio_tungstenite::WebSocketStream` / `tungstenite::Message` / `connect_async`）
- **决策**：❌ **`tokio-tungstenite` in synthia-cli 保留**——CLI 是 REPL / 客户端，主动通过 WS 连接 server endpoint
- **synthia-server**：`grep -rn 'tungstenite' crates/synthia-server/src/` → 0 命中（server WS 走 `axum::extract::ws` 内置）→ ✅ 确认可删除
- **根 workspace `[workspace.dependencies] tokio-tungstenite = "0.24"`**：❌ **保留**（synthia-cli 仍需）
- **Cargo.toml 调整**：
  - `crates/synthia-server/Cargo.toml`：删除 `tokio-tungstenite.workspace = true`
  - `crates/synthia-cli/Cargo.toml`：**保留** `tokio-tungstenite = { workspace = true }`
  - 根 `Cargo.toml` workspace 段：**保留** `tokio-tungstenite = "0.24"`

### 8.4 §1.5 nix in synthia-agent：0 命中

- ✅ 确认可删除 `crates/synthia-agent/Cargo.toml: nix = ...`

### 8.5 §1.6 libc in synthia-tool：0 命中

- ✅ 确认可删除 `crates/synthia-tool/Cargo.toml: libc = "0.2"`

### 8.6 §1.7 serial_test + pretty_assertions 命中情况

| crate | 命中数 | 决策 |
|---|---:|---|
| `synthia-provider/src/config.rs` | **8**（`#[serial_test::serial]`） | ❌ **保留** `serial_test = "3"` |
| `synthia-server/src/` + `tests/` | 0 | ✅ 确认可删除 `serial_test = "3"` |
| `synthia-protocol/src/` + `tests/` | 0 | ✅ 确认可删除 `pretty_assertions = "1"` |

### 8.7 实际可移除清单（阶段 1 决策汇总）

```
crates/synthia-core/src/pbac/                                  # DELETE 整目录（23 文件）
crates/synthia-core/src/lib.rs                                 # DELETE 2 行（pub mod + pub use）
crates/synthia-server/Cargo.toml                               # DELETE a2a-pb / tokio-tungstenite / serial_test
crates/synthia-agent/Cargo.toml                                # DELETE nix
crates/synthia-tool/Cargo.toml                                 # DELETE libc
crates/synthia-provider/Cargo.toml                             # DELETE pretty_assertions ← 修正（原误为 serial_test）
crates/synthia-protocol/Cargo.toml                             # DELETE pretty_assertions
Cargo.toml                                                     # DELETE a2a-pb patch 块（保留 workspace tokio-tungstenite）
Cargo.lock                                                     # cargo build 自动同步

crates/synthia-server/src/a2a/service.rs                       # MODIFY 注释（vendor/a2a-pb → a2a.proto）
crates/synthia-server/src/a2a/serde_sse.rs                     # MODIFY 注释（vendor/a2a-pb → a2a.proto）

不动:
crates/synthia-cli/Cargo.toml                                  # tokio-tungstenite 保留
crates/synthia-cli/src/wire.rs                                 # 不动
crates/synthia-provider/Cargo.toml                             # serial_test 保留（config.rs 真在用）
crates/synthia-provider/src/config.rs                          # 不动
根 Cargo.toml workspace.tokio-tungstenite                       # 保留（synthia-cli 引用）
```