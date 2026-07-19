# Retrospective: adversarial-audit-p0-fixes

> Written: 2026-06-29 (after verify passed)
> Commit range: `7f591d6..c80de27` (5 commits)
> Worktree: `/home/crochee/workspace/synthia/.worktrees/adversarial-audit-p0-fixes`

---

## 0. Evidence

> 量化前置數據 — 後續 Wins / Misses bullets 直接引用,避免每行重複 [evidence: ...]。

- **Commit range**: `7f591d6..c80de27` (5 commits)
- **Diff size**: +1129 / -110 lines across 21 files
- **Tasks done**: 48/48 (`grep -cE '^\s*- \[x\]' tasks.md` → 48)
- **Active hours**: ~6 hours (探索阶段已在前置 conversation 完成,本 cycle 仅 apply + verify)
- **Subagent dispatches**: ~17 (5 tasks × (1 implementer + 1 spec reviewer + 1 code reviewer) + 2 amend 后 re-review)
- **New external dependencies**: none (仅复用现有 `synthia-sandbox`, `synthia-permission` 等 crate)
- **Bugs encountered post-merge**: 0 (尚未 merge to master,待 finishing-a-development-branch)
- **OpenSpec validate state at archive**: pass (`openspec validate adversarial-audit-p0-fixes --strict` exit 0)
- **Test coverage signal**: `cargo test --workspace` exit 0,全部通过;新增 ~15 个测试(bash sandbox 3 + path containment 7 + prefix hash 6 + pipeline 2)

Commit chain (時序):

```
4f3f188 fix(context): replace O(n²) remove(0) with O(n) drain in ProtectionZone trim  (Task 1)
514d3c1 fix(guardian): inject CachePolicy::default() to eliminate ~$19.7K/yr cache miss waste  (Task 2)
9238f68 fix(sandbox): wire SandboxAttempt::wrap into bash executor to close single-point-of-failure  (Task 3, amended from 11c5ebe)
1e8c185 fix(permission): replace ../ substring check with workspace containment to prevent absolute path escape  (Task 4, amended from 850ee5d)
c80de27 fix(context): extend PrefixTracker hash to cover system+tools+messages for accurate stability_ratio  (Task 5)
```

---

## 1. Wins

- [evidence: commit 4f3f188 + crates/synthia-context/src/assembler/pipeline.rs] Task 1 O(n²) → O(n) drain 修复一次通过 spec + code review,无 amend。算法清晰:`msg_tokens` 预计算 + 后缀和递减 + 单次 `drain(0..remove_count)`,200+ 消息时 100-250ms 延迟消除。
- [evidence: commit 514d3c1 + reviewer.rs:218,314 + e2e_llm_test.rs:121,201] Task 2 cache_policy 注入是单点修复(~3 行/文件),但 ROI 巨大(~$19.7K/年)。一次通过 review。
- [evidence: commit 9238f68 + executor.rs `build_bash_command`] Task 3 引入 `Tool::call_with_sandbox` trait default method 是正确的关注分离——`Tool::call` 保持兼容旧 caller,`call_with_sandbox` 让需要沙箱的 caller(BashTool)显式 override。spec compliance review 发现 Scenario 3 (Allow policy) 未实现后,amend 一次补齐 `SandboxAttempt::None` → `tracing::warn!("sandbox disabled by policy")` 映射。
- [evidence: commit 1e8c185 + checker.rs `is_path_in_workspace`] Task 4 引入 fail-closed 检查(`workspace_normalized.as_os_str().is_empty()` 时返回 false)是关键安全修复。原方案漏掉此点,spec compliance review 发现后 amend 补齐。
- [evidence: commit c80de27 + tracker.rs `compute_hash_bytes`] Task 5 hash 扩展到 `system || tools || messages_prefix` 固定顺序拼接,新增 `canonical_tools_schema_bytes` 和 `canonical_messages_prefix_bytes` 两个辅助函数。12 个现有测试更新 + 6 个新测试,一次通过 review。
- [evidence: tasks.md 48/48 + verify.md PASS] 全部 5 个 task 通过 spec compliance + code quality 双重 review,集成验证全绿(`cargo test` + `fmt` + `clippy` + `openspec validate --strict`)。

## 2. Misses

- 🔴 [blocking | evidence: Task 4 amend `850ee5d` → `1e8c185`] **fail-open 安全漏洞**:`default_workspace_root()` 在 `current_dir()` 失败时回退到 `PathBuf::from(".")`,`normalize_path(".")` 产生空 PathBuf,`Path::starts_with("")` 对任意路径返回 true → 所有绝对路径逃逸被放行。spec compliance review 捕获,amend 修复为 fail-closed。**根因**:设计阶段未考虑 `current_dir()` 失败的 fallback 路径。
- 🟡 [painful  | evidence: code review Task 3 follow-up] **前台/后台 deny 错误消息前缀不一致**:前台 `execute_command` 返回 `"bash execution denied to prevent bare-run"`,后台 spawn 路径错误消息不同。**根因**:两条路径的错误构造分散在不同函数。
- 🟡 [painful  | evidence: code review Task 3 follow-up] **`Tool::call` 默认委派到 `call_with_sandbox(None)` 是文档化 footgun**:trait default method 用 `let _ = sandbox_attempt;` 显式忽略 sandbox,意味着任何未 override `call_with_sandbox` 的 Tool 都会裸跑。应在 doc comment 中明确警告。
- 🟡 [painful  | evidence: code review Task 4 follow-up] **`extract_string_values` 递归提取会误判**:严格化检查后 `write_file` 写入含绝对路径字符串的内容会被误判为逃逸。`subagent/config.rs` 与 `agent_tool.rs` 未调用 `with_workspace_root`,子代理静默回退到 cwd 语义。**根因**:U2 修复范围限于 `PermissionChecker`,未覆盖所有 caller。
- 🟡 [painful  | evidence: code review Task 5 follow-up] **`unwrap_or_default()` 静默吞序列化错误**:`canonical_tools_schema_bytes` 序列化失败时返回空 Vec,导致 hash 不变 → 掩盖 tools 变更。`record_post` 返回值在 `main_loop.rs` 中被丢弃,无法传播错误。
- 📌 [nit      | evidence: Task 3 amend] **SandboxPolicy::Allow 变体映射缺失**:原方案 spec 中提到 "policy Allow" 场景,但实现时未映射 `SandboxPolicy::None` ≡ spec 的 "Allow"。spec compliance review 发现后 amend 补齐 `tracing::warn!`。
- 📌 [nit      | evidence: Task 5 test] **`test_stable_prefix_reports_high_stability` 含无效断言**:测试断言 `stability_ratio > 0.0`,但 ratio 永远 ≥ 0,断言无意义。

## 3. Plan deviations

| Plan task | What changed | Why |
|-----------|--------------|-----|
| Task 3 | amend commit `11c5ebe` → `9238f68`:补齐 `SandboxAttempt::None` 的 `tracing::warn!` 日志 + 后台 spawn 路径测试 | spec compliance review 发现 Scenario 3 (Allow policy) 未实现 + Scenario 4 无测试覆盖 |
| Task 4 | amend commit `850ee5d` → `1e8c185`:新增 `is_path_in_workspace` 的 fail-closed 检查 | spec compliance review 发现 `current_dir()` 失败回退到 "." 会导致所有绝对路径逃逸被放行(High 严重性 fail-open 漏洞) |
| Task 5 | 12 个现有测试需更新签名(传 `&[], &[]` 保留原语义) + 6 个新测试 | `record_pre`/`record_post` 签名从 1 参扩展到 3 参,所有 caller 需同步更新 |

## 4. Skill / workflow compliance

| Skill                                            | Used |
|--------------------------------------------------|------|
| superpowers:brainstorming                        | ✓ (前置 conversation explore 阶段产出 brainstorm.md) |
| superpowers:writing-plans                        | ✓ (plan.md TDD 微步骤实现计划) |
| superpowers:using-git-worktrees                  | ✓ (worktree 在 `.worktrees/adversarial-audit-p0-fixes`) |
| superpowers:subagent-driven-development         | ✓ (5 tasks × implementer + spec reviewer + code reviewer + amend re-review) |
| (transitive) superpowers:test-driven-development | ✓ (每个 task 新增测试,集成验证 `cargo test --workspace` 通过) |
| (transitive) superpowers:requesting-code-review   | ✓ (spec compliance + code quality 双重 review) |
| superpowers:finishing-a-development-branch        | ✗ (待 archive 后执行) |

### Deliberately Skipped Skills

- **`superpowers:finishing-a-development-branch`**
  - **What was skipped**: 整个 skill 暂未执行
  - **Why this cycle**: 本 retro 写于 archive 之前。apply 指令明确要求 archive 在前、finishing-a-development-branch 在后("After retrospective + archive are both done, use the Skill tool to invoke superpowers:finishing-a-development-branch")。这是 superpowers-bridge schema 的设计顺序,不是异常跳过。
  - **How to prevent recurrence**: `one-off — schema boundary case, no prevention possible`。finishing-a-development-branch 在 apply 工作流中是最后一步,retro 写于它之前是 schema 设计;此 skill 将在本 retro 写完后立即执行,无需预防。

## 5. Surprises

- **原以为 U2 路径校验只需替换 `../` 子串检查为 `normalize()` + `starts_with`**,实际发现 `current_dir()` 失败回退到 "." 会导致 `normalize_path(".")` 产生空 PathBuf,`Path::starts_with("")` 对任意路径返回 true → fail-open。这是 codex safety.rs 未显式处理的边界情况(codex 假设 workspace_root 非空)。
- **原以为 prefix_tracker hash 扩展只需改 `compute_hash_bytes` 一行**,实际发现需要新增 `canonical_tools_schema_bytes`(按 name 排序)和 `canonical_messages_prefix_bytes`(`take_while` 到第一个 `tool_result_cleared_at`)两个辅助函数,且 12 个现有测试需同步更新签名。改动范围比预期大 3 倍。
- **原以为 bash 沙箱只需在 executor 加 `wrap` 调用**,实际发现需要:(1) 新增 `Tool::call_with_sandbox` trait default method 让 sandbox 透传不影响其他 Tool;(2) 后台 spawn 路径也需覆盖(原代码用裸 `Command::new("bash")`);(3) spec 场景 3 (Allow policy) 需映射 `SandboxPolicy::None` 并加 `tracing::warn!`。
- **`openspec/` 目录被 `.gitignore` 忽略**:`git status` 显示 clean 但文件存在于磁盘。worktree 通过 symlink 访问 openspec 变更工件。这意味着 tasks.md / verify.md / retrospective.md 的修改不会进 git,但 openspec CLI 会读它们。

## 6. Promote candidates → long-term learning

- [ ] 🔴 **PermissionChecker 所有 fallback 必须默认 fail-closed** → **Promote to memory** (type: feedback)
  > **Why**: Task 4 amend 发现 `current_dir()` 失败回退到 "." 导致 `is_path_in_workspace` 对任意路径返回 true,所有绝对路径逃逸被放行。这是 High 严重性 fail-open 漏洞,被 spec compliance review 捕获。
  > **How to apply**: 在 `crates/synthia-permission/src/checker/checker.rs` 的任何新 fallback 路径中,若 workspace_root 归一化为空,必须返回 false(Deny)而非继续校验。适用所有 `is_path_in_workspace` / `check_path_traversal` 类函数。

- [ ] 🟡 **`Tool::call` 默认委派到 `call_with_sandbox(None)` 需文档 warning** → **Promote to skill** (`subagent-driven-development` 的 implementer-prompt.md)
  > **Why**: Task 3 引入的 trait default method 用 `let _ = sandbox_attempt;` 显式忽略 sandbox,任何未 override `call_with_sandbox` 的 Tool 都会裸跑。这是文档化 footgun。
  > **How to apply**: 在 implementer-prompt.md 的 "Code Organization" 段加一行:"新增 Tool trait method 时,若默认实现忽略安全相关参数(如 sandbox_attempt),必须在 doc comment 中加 `# Warning` 段说明 override 责任。"

- [ ] 🟡 **`unwrap_or_default()` 静默吞序列化错误会掩盖可观测性问题** → **Promote to memory** (type: feedback)
  > **Why**: Task 5 `canonical_tools_schema_bytes` 序列化失败时返回空 Vec,导致 hash 不变 → 掩盖 tools 变更 → stability_ratio 虚高。
  > **How to apply**: 在 hash 计算或 fingerprint 相关代码中,序列化失败应返回 `Result` 或 panic,而非 `unwrap_or_default()`。适用 `crates/synthia-context/src/prefix_tracker/` 全部函数。

- [ ] 🟡 **spec compliance review 应在 implementation 中段就检查 spec 文件** → **Promote to skill** (`subagent-driven-development` 的 process)
  > **Why**: Task 3 和 Task 4 都因 spec compliance review 后置发现漏洞才 amend,若 implementation 阶段中段就对照 spec scenarios 自检,可避免 amend 往返。
  > **How to apply**: 在 implementer-prompt.md 的 "Self-Review" 段加一步:"对照 spec 文件的每个 Scenario,逐条确认实现是否覆盖。若某 Scenario 无对应代码路径,标记为 DONE_WITH_CONCERNS 并说明。"

- [ ] 📌 **`extract_string_values` 递归提取 JSON 字符串会误判 write_file 内容** → **One-off** (记录即可,不 promote)
  > **Why**: U2 修复范围限于 `PermissionChecker`,但 `write_file` 写入含绝对路径字符串的内容会被 `extract_string_values` 误判为逃逸。这是 PermissionChecker 与 Tool input schema 的边界问题,不通用化。

- [ ] 📌 **OpenSpec `openspec/` 被 `.gitignore` 忽略导致工件不进 git** → **One-off** (记录即可,不 promote)
  > **Why**: `.gitignore` 第 37 行 `openspec` 忽略整个 openspec 目录。tasks.md / verify.md / retrospective.md 的修改不会进 git,但 openspec CLI 会读它们。这是项目配置选择,非通用问题。
