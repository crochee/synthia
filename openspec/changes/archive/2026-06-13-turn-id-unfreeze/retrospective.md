# Retrospective: turn-id-unfreeze

> Written: 2026-06-13 (after meta-change completion, before archive)
> Schema: superpowers-bridge

---

## 0. Evidence

- **Change type**: META-CHANGE (0 code modifications, 0 crate changes)
- **Artifacts**: 8/8 complete
- **OpenSpec validate**: pass
- **Commit pending**: 1 commit (`docs(openspec): record turn-id-mvp unfreeze trigger`)
- **Archive pending**: 1 manual archive to `openspec/changes/archive/2026-06-13-turn-id-unfreeze/`
- **Subagent dispatches**: 0 (single-agent, meta-change scope)
- **Bugs encountered**: 0 (no implementation to break)
- **Skill / workflow compliance**: brainstorming ✅ + writing-plans ✅ + verification-before-completion ✅

---

## 1. Wins

- [evidence: codex PR #28002+#27996 引用 + 2296+391+349+241 行表] **外部工业级证据被永久记录**：proposal.md 直接引用 PR 描述原文，design.md 量化 codex 投资规模（3000+ 行 / 5+ 模块），未来解冻时无需再次论证"按 turn 维度查询"是否为真实需求
- [evidence: D2 决议显式拒绝立即解冻] **保持"speculative architecture 应被推迟"项目原则的克制**：codex 工业级证据强 + 3 个月观察窗口有价值 = 维持冻结期不缩短。避免"破窗效应"
- [evidence: 8 个 artifacts / 1 commit / 0 代码] **元变更形式与 OpenSpec 元数据层天然契合**：本 change 的产出 100% 限于 `openspec/changes/turn-id-unfreeze/` 目录，决策链可追溯且不污染 FROZEN 状态
- [evidence: 6 个 Requirement / ≥ 6 个 Scenario 全部带 SHALL/MUST] **OpenSpec 形式化合规**：每个 Requirement 第一句包含 SHALL/MUST，每个 Requirement 至少 1 个 Scenario（WHEN/THEN 格式）
- [evidence: D1 决议定义"元变更"边界] **元变更边界明确化**：本 change ❌ 不创建 `turn.rs` / ❌ 不修改 `LoopContext` / ❌ 不修改 `StreamBuilder` / ❌ 不修改 `synthia-hook` —— 实施仍归 `turn-id-mvp` change
- [evidence: D4 决议列举 3 个前置条件] **3 个前置条件门控显式记录**：unify-token-usage-types / turn-id-unify / recovery-path-explicit 任一未完成时不解冻，避免引入第 6 个 turn_id 表示
- [evidence: D3 决议禁止 codex 模块复制] **codex 设计仅作 reference**：解冻后实施 MVP 时可参考 codex 工业级语义，但 Synthia 仍走简化派 MVP（~20 行）而非 codex 全量（3000+ 行）
- [evidence: D6 决议以 PR 链接 + commit hash 记录触发证据] **可追溯性强**：未来审阅者可直接点击 PR 链接验证 codex 工业级证据的真实性，无需二手转述

---

## 2. Misses

- 📌 [evidence: 元变更不实施代码，无"代码方案"可审查] **元变更不需走 4 派对抗性审查**：本 brainstorm.md Q4 显式论证"复用 turn-id-mvp 4 派审查 + 本档 4 题论证元变更决策点"是合理替代，但严格来说"4 派审查"被替换为"4 题论证"是流程的轻微降级
- 📌 [evidence: design.md §Open Questions Q1-Q4 仍为 open] **Q1-Q4 仍是 open 状态**：(Q1) codex design notes 是否必做；(Q2) 监控 codex PR 频率；(Q3) codex 引入 persistence 时 Synthia 是否同步；(Q4) 是否需要 `.openspec.yaml`。这些"未决议"未在元变更 scope 内关闭，留作冻结期监控时处理
- 📌 [evidence: `openspec/` 目录是 gitignored] **元变更的 commit 实际为空**：由于 `openspec/` 全部 gitignored，本 change 提交 commit 后实际仅携带"0 代码变更"的元数据证明。提交 commit 主要是为了"留下决策时点的 git 记录"，而 `openspec/` 文件本身是 local-only 的

---

## 3. Plan deviations

| Plan task | What changed | Why |
|-----------|--------------|-----|
| 1.1 (brainstorm) | 元变更不需走完整 4 派审查 → 4 题论证替代 | D1 决议定义元变更边界后，4 派审查不适用（无代码方案可审查） |
| 1.6 (plan) | 4-step 流程替代 25+ 任务分解 | 元变更仅"写文件 + 提交 + 归档"，无需 TDD 风格的 micro-step 拆分 |
| 2 (验证) | 简化验证检查（无 cargo test / clippy） | 0 代码变更，验证仅限 OpenSpec + git diff |
| 4 (归档) | 手动 archive 而非 `openspec archive` | 已知 `openspec archive` 在 spec 已同步时 abort；手动 archive 与 apply-patch-tool 流程一致 |
| 5 (监控) | 监控范围限定于 codex turn.rs history | Q2 决议"每周一次"是设计选择，未在 plan 中显式说明 |

---

## 4. Skill / workflow compliance

| Skill | Used |
|-------|------|
| `superpowers:brainstorming` | ✓ (本档 §Brainstorm 4 题替代完整 4 派审查，因元变更不实施代码) |
| `superpowers:writing-plans` | ✓ (plan.md 6 task groups: artifacts + verify + commit + archive + monitor + deadline) |
| `superpowers:verification-before-completion` | ✓ (verify.md 5 sections: evidence / compliance / impact / spec sync / open) |
| `superpowers:test-driven-development` | N/A (0 code → 0 tests) |
| `openspec validate --strict` | ✓ pass |
| `openspec status --change` | 8/8 artifacts |

---

## 5. Open follow-ups

- **codex design notes** (Q1, optional): 解冻后实施 TurnId MVP 时，可阅读 codex 2296 行 `turn.rs` 后写 "synthia-vs-codex Turn design notes" markdown。**不强制**，仅作 reference notes
- **监控 codex 后续 PR** (Q2, 每周一次): 2026-06-13 → 2026-09-13 期间监控 `codex-rs/core/src/session/turn.rs` 是否有重大变更
- **3 个前置条件追踪** (D4): 每周 `openspec list | grep -E "unify-token-usage-types|turn-id-unify|recovery-path-explicit"` 跟踪进度
- **2026-09-13 thaw assessment**: 当日评估 3 个 thaw 条件状态，决定是否解冻 `turn-id-mvp`
- **2026-12-13 hard deadline**: 如仍未解冻 → 归档到 `archive/turn-id-mvp-expired/`，标注 `turn-id-label` capability 为 "deferred indefinitely"

---

## 6. Decision: archive this change

The meta-change is fully documented and ready for archive:

- 8/8 OpenSpec artifacts complete and validated
- 0 code modifications verified via `git diff --stat crates/`
- `turn-id-mvp/` directory not modified (frozen state preserved)
- codex PR #28002+#27996 evidence permanently recorded
- 3-month freeze period decision formalized (D2 resolution)

**Action**: manually archive to `openspec/changes/archive/2026-06-13-turn-id-unfreeze/` (per apply-patch-tool pattern) and sync delta spec to `openspec/specs/turn-id-unfreeze/spec.md` (cumulative format). Then `rm -rf openspec/changes/turn-id-unfreeze/`. Since `openspec/` is gitignored, no commit is required for the archive itself.

Future retrieval: when `turn-id-mvp` is eventually thawed (2026-09-13 or after 3 prerequisites complete), the implementation agent should read `openspec/changes/archive/2026-06-13-turn-id-unfreeze/retrospective.md` (this file) to recall the meta-change rationale, then proceed with `turn-id-mvp/tasks.md` 2.1-2.6 implementation steps.
