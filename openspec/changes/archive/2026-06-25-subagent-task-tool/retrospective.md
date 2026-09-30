# Retrospective: subagent-task-tool

> Written: 2026-06-25 (after verify passed)
> Commit range: `cafdb99~1..cafdb99`
> Worktree: 已合并到 master（worktree 已清理）

---

## 0. Evidence

> 量化前置數據 — 後續 Wins / Misses bullets 直接引用。

- **Commit range**: `cafdb99~1..cafdb99` (1 commit)
- **Diff size**: +1056 / -138 lines across 21 files
- **Tasks done**: 36/36 (`grep -cE '^\s*- \[x\]' tasks.md` → 36)
- **Active hours**: 无法精确估算（单 commit，实现跨多个会话）
- **Subagent dispatches**: 无上下文记录（本 retrospective 写于实现完成后，无实时实施日志）
- **New external dependencies**: none（Cargo.toml 无新增 dependency）
- **Bugs encountered post-merge**: none（已合并到 master，测试通过）
- **OpenSpec validate state at archive**: pass (for this change; 4 pre-existing spec failures unrelated)
- **Test coverage signal**: agent_tools/tests.rs (+62 lines), subagent_integration_test.rs (+85 lines) 更新

Commit chain (時序):

```
cafdb99 feat(agent,server,cli): implement subagent task tool with ForkPolicy, permissions, and background execution
```

---

## 1. Wins

- [evidence: cafdb99 diff stat] 单 commit 完成完整功能：21 files, +1056 lines，覆盖 agent/server/cli 三个层，实现内聚度高。
- [evidence: 7 spec capabilities] 设计完整：7 个 capability spec（subagent-background-mode, subagent-built-in-types, subagent-event-bridge, subagent-permission-inheritance, subagent-session-model, subagent-task-tool, tool-execution），5 个 design 决策（D1-D5）全部有对应 spec requirements。
- [evidence: design.md D3 + spec] 权限继承模型设计清晰：deny-only 继承 + default-deny task/todowrite，符合 agent_rule.md P6（不信任 LLM）原则——子 agent 默认不能调用 task tool 或 todowrite。
- [evidence: spec headers] 所有 7 个 change spec 使用正确的 `## ADDED Requirements` / `## MODIFIED Requirements` header，符合项目硬约束。
- [evidence: tool-execution spec] 向后兼容性考虑：`build_default_tool_registry` 接受可选 subagent 依赖，无 subagent 基础设施的调用者不受影响。
- [evidence: verify.md §4] Design 与 specs 完全对齐，无漂移。

## 2. Misses

- 🟡 [painful | evidence: 4 pre-existing spec failures] `subagent-event-bridge` 和 `subagent-session-model` 的主 spec 缺少 `## Purpose` section（验证失败）。本 change 的 delta spec 使用 `## ADDED Requirements` / `## MODIFIED Requirements` header（正确），但 archive 后合并到主 spec 时，`## Purpose` 缺失问题仍然存在。这是 pre-existing 问题，不阻塞本 change archive，但应在后续修复。
- 📌 [nit | evidence: retrospective 写于实现后] 本 retrospective 写于实现完成后的归档阶段，缺少实施过程的实时上下文（是否有 subagent dispatch、是否有 misses、是否有 plan deviations）。如需更准确的 retrospective，应在实现完成后立即编写。
- 📌 [nit | evidence: 1 commit] 所有 21 files 的修改在 1 个 commit 中，commit 粒度较大。虽然 commit message 描述详细（7 个 bullet points），但如需 bisect 或 revert 部分功能，粒度不够细。

## 3. Plan deviations

| Plan task | What changed | Why |
|-----------|--------------|-----|
| — | 无记录 | 本 retrospective 写于实现后，无实时的 plan deviation 记录 |

## 4. Skill / workflow compliance

| Skill                                            | Used |
|--------------------------------------------------|------|
| superpowers:brainstorming                         | ✓ (prior phase) |
| superpowers:writing-plans                         | ✓ (prior phase) |
| superpowers:using-git-worktrees                  | ✓ (worktree 已清理) |
| superpowers:subagent-driven-development          | ? (无实时记录) |
| (transitive) superpowers:test-driven-development | ? (无实时记录) |
| (transitive) superpowers:requesting-code-review  | ? (无实时记录) |
| superpowers:finishing-a-development-branch       | ✓ (已合并到 master) |

> **Note**: 本 change 的实施阶段在之前的对话中完成，缺乏实时的 skill compliance 记录。基于 worktree 已存在并清理、实现已合并到 master 的事实，推断 using-git-worktrees 和 finishing-a-development-branch 已执行。subagent-driven-development 及其 transitive skills 是否执行无法确认。

### Deliberately Skipped Skills

- **无法确认** — 本 retrospective 写于实现完成后，无实施阶段的实时日志。如果 subagent-driven-development 的两阶段审查被跳过，这与 p0-reliability-security-fixes 的 pattern 一致（见该 change 的 retrospective §4）。

## 5. Surprises

- **7 个 capability specs**：本 change 涉及 7 个 capability 目录，比预期多。这反映了 subagent task tool 的跨层特性——它同时修改了 tool-execution（MODIFIED）、subagent-session-model（MODIFIED），并创建了 5 个新 capability。
- **无新外部依赖**：尽管实现了进程间通信和 background execution，没有引入新的外部 crate。这是好的——复用了现有 tokio 生态。

## 6. Promote candidates → long-term learning

- [ ] 🟡 **主 spec 缺少 `## Purpose` section 的 pre-existing 问题应批量修复** → **Promote to memory** (type: feedback)
  > **Why**: 4 个主 spec（subagent-event-bridge, subagent-listing, subagent-session-model, v2-session-api）缺少 `## Purpose` section，导致 `openspec validate` 失败。每次 verify 都需要解释这些是 pre-existing，增加了认知负担。
  > **How to apply**: 下次修改这些 spec 时，顺便添加 `## Purpose` section。或安排一个专门的 spec 修复 change。

- [ ] 📌 **Retrospective 应在实现完成后立即编写** → **Promote to CLAUDE.md** (`<project CLAUDE.md>` 段)
  > **Why**: 本 retrospective 写于实现完成后，缺少实施过程的实时上下文（misses、plan deviations、skill compliance）。retrospective 的价值在于捕获"当时知道什么"，事后补写会丢失这些。
  > **How to apply**: 在 openspec-apply-change 的流程中，retrospective 必须在 verify 通过后立即编写，不应推迟到归档阶段。

- [ ] 📌 **大 commit 粒度应考虑拆分** → **Promote to memory** (type: convention)
  > **Why**: 21 files / +1056 lines 在 1 个 commit 中，commit message 有 7 个 bullet points，说明这是一个复合改动。如需 bisect 或 revert 部分功能，粒度不够细。
  > **How to apply**: 当 commit message 超过 3 个独立 bullet points 时，考虑拆分为多个 commit，每个对应一个独立的功能单元。
