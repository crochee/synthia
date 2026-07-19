# turn-id-unfreeze Implementation Plan (META-CHANGE)

> **重要：本 change 是元变更（meta-change），0 代码变更。**
> **本 plan.md 仅记录"写 4 个 artifact + 1 个 commit + 1 个 archive"流程。**
> **TurnId MVP 实施仍归 `turn-id-mvp/tasks.md`（受 3 个前置条件门控，解冻后 2026-09-13 起执行）。**

**Goal:** 把"codex 触发的解冻事件"以 OpenSpec 元数据形式记录，让 `turn-id-mvp` 状态变化有完整可追溯的决策链。**0 代码变更，0 实施风险。**

**Architecture:** 不修改任何 crates 代码；不修改 `turn-id-mvp/` 目录；不创建新 source 文件；不修改 `LoopContext` / `StreamBuilder` / `synthia-hook::AgentContext`。本 change 的产出 100% 限于 `openspec/changes/turn-id-unfreeze/` 目录下的 8 个 OpenSpec artifacts。

**Tech Stack:** OpenSpec, Markdown, Git

---

## 任务列表

### 1. 完成 8 个 OpenSpec artifacts

- [x] 1.1 `brainstorm.md`（4 派论证 + 元变更决策点）
- [x] 1.2 `design.md`（D1-D6 决议）
- [x] 1.3 `proposal.md`（Why/What Changes/Capabilities/Impact）
- [x] 1.4 `specs/turn-id-unfreeze/spec.md`（6 个 ADDED Requirements + Scenarios）
- [x] 1.5 `tasks.md`（7 个 task group：记录 + 评估 + 形式化 + 验证 + 监控 + 衔接 + 硬截止）
- [x] 1.6 `plan.md`（本档）
- [ ] 1.7 `verify.md`（验证 OpenSpec pass + 0 代码变更）
- [ ] 1.8 `retrospective.md`（元变更流程经验总结）

### 2. 验证

- [ ] 2.1 运行 `openspec validate turn-id-unfreeze --type change --strict` 期望通过
- [ ] 2.2 运行 `openspec show turn-id-unfreeze` 期望显示 4-8 个 artifact 全部存在
- [ ] 2.3 验证 `turn-id-mvp/` 目录未被修改：
  - `git status openspec/changes/turn-id-mvp/` 期望 "nothing to commit"
  - `git diff openspec/changes/turn-id-mvp/` 期望空输出
- [ ] 2.4 验证本 change 0 代码变更：
  - `git diff --stat` 仅显示 `openspec/changes/turn-id-unfreeze/` 下的文件
  - `git diff --stat crates/` 期望空输出
  - `git diff --stat` 不应出现 `crates/synthia-agent/src/turn.rs` / `loop_context.rs` / `stream_builder/builder.rs`

### 3. 提交

- [ ] 3.1 提交 commit：
  ```bash
  git add openspec/changes/turn-id-unfreeze/
  git commit -m "$(cat <<'EOF'
  docs(openspec): record turn-id-mvp unfreeze trigger
  
  Record OpenAI codex PR #28002 and #27996 (both merged 2026-06-13)
  as evidence that the "concrete use case" unfreeze condition for
  turn-id-mvp has been met. Maintain the 3-month freeze period
  (2026-06-13 → 2026-09-13) and keep implementation gated by the
  three prerequisite changes (unify-token-usage-types, turn-id-unify,
  recovery-path-explicit). This is a meta-change with zero code
  modifications.
  EOF
  )"
  ```
- [ ] 3.2 推送：等用户确认后再推送

### 4. 手工归档（参照 apply-patch-tool 流程）

- [ ] 4.1 复制 8 个 artifacts 到 `openspec/changes/archive/2026-06-13-turn-id-unfreeze/`
- [ ] 4.2 同步 delta spec 到 `openspec/specs/turn-id-unfreeze/spec.md`（cumulative 格式：`## Requirements`，无 `ADDED` 前缀）
- [ ] 4.3 创建 `openspec/changes/archive/2026-06-13-turn-id-unfreeze/.openspec.yaml` (`schema: superpowers-bridge`)
- [ ] 4.4 创建 `openspec/changes/archive/2026-06-13-turn-id-unfreeze/README.md`（简要说明）
- [ ] 4.5 运行 `openspec spec validate turn-id-unfreeze --strict` 期望通过
- [ ] 4.6 `rm -rf openspec/changes/turn-id-unfreeze/`

### 5. 冻结期监控（2026-06-13 → 2026-09-13，归档后由 `turn-id-mvp/tasks.md` 接管）

- [ ] 5.1 监控 codex 后续 PR（每周一次 `git -C codex fetch && git log --since=... -- codex-rs/core/src/session/turn.rs`）
- [ ] 5.2 监控 3 个前置条件完成进度：
  - `unify-token-usage-types`
  - `turn-id-unify`
  - `recovery-path-explicit`
- [ ] 5.3 等待 2026-09-13 硬解冻日

### 6. 6 个月硬截止（2026-12-13）

- [ ] 6.1 如 2026-09-13 时前置条件仍未完成：归档 `turn-id-mvp/` 到 `archive/turn-id-mvp-expired/`
- [ ] 6.2 `turn-id-label` capability 标注 "deferred indefinitely"

---

## 验收条件

- [ ] `openspec status --change turn-id-unfreeze` 显示 8/8 artifacts complete
- [ ] `openspec validate turn-id-unfreeze --type change --strict` 通过
- [ ] `openspec spec validate turn-id-unfreeze --strict` 通过（归档后）
- [ ] 0 代码变更（`git diff --stat crates/` 空输出）
- [ ] `turn-id-mvp/` 目录未被修改
- [ ] commit message 明确标注 "META-CHANGE / 0 code modifications"
- [ ] 归档目录含 `.openspec.yaml` + `README.md` + 8 个 artifacts + `specs/`
