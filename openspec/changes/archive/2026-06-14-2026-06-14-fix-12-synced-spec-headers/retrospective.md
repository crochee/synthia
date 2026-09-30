# Retrospective: Fix 12 Synced Spec Headers + Add Format Drift CI Gate

> **Date**: 2026-06-14
> **Change**: 2026-06-14-fix-12-synced-spec-headers
> **Outcome**: All 12 specs pass strict validation, CI gate installed, 0 source code changes

---

## 1. What went well

### 1.1 苏格拉底拆解发现 6 项 design bug
- 4 派审查不仅是仪式——brainstorm 阶段发现了 design.md 6 处错误（Pattern A/B 数量错、`recovery-cascade-wiring` 错分、spec.md 笔误、数字错）
- 没有这一轮审查就会带着错误 design 进入实施,导致 1-2 个 spec 修复不全
- **Lesson**: 即使 "simple" 任务也值得走 4 派审查 — 4 个独立 lens 在 5 分钟内能发现 1 个开发者遗漏的 bug

### 1.2 Pattern A/B 分类作为决策框架
- 把 12 specs 拆成两类（rename-only vs rename+prepend）让实施可以**并行/批量**
- 验证了: 1 个分类错误 = 1 个实施错误 = 1 个回归
- **Lesson**: 修复任务如果涉及 ≥5 个类似项,先分类再实施

### 1.3 CI gate 复用现有 pattern
- `scripts/check_reexports.sh` 已存在 (synthia-session re-export 3-layer defense 的第 3 层)
- 新脚本沿用其风格 (header comment + usage + exit code)
- **Lesson**: 仓库内已有的同类脚本是最佳 reference

### 1.4 openspec 工具 + openspec-spec-format 分工
- `openspec spec validate --strict` 是 spec 合规性检查
- `bash scripts/check_synced_spec_format.sh` 是结构性 format 检查
- 两者独立,失败信号不同(spec 内容问题 vs 格式漂移)
- **Lesson**: 不同的 lint 工具,不同的用途,不要混用

---

## 2. What went wrong

### 2.1 openspec CLI 1.3.1 不一致
- `list` 和 `validate` 接受 date-prefixed change name (`2026-06-14-...`)
- `status` 和 `instructions apply` 拒绝 ("must start with a letter")
- 影响: openspec-apply-change skill 的 status/instructions 命令无法使用
- **Workaround**: 手动按 tasks.md 实施
- **Lesson**: 工具 bug 不要"硬卡",但要记录为后续 issue

### 2.2 grep -E 语法混淆
- 初始 pattern 用了 `\(ADDED\|MODIFIED\)` (BRE 语法) 加 `-E` (ERE 模式)
- GNU grep -E 把反斜杠当作 literal,导致 0 匹配 (silent failure)
- 自验证 (合成 drift 文件) 才发现问题
- **Lesson**: bash regex 默认用 POSIX BRE; 用 -E 时切到 ERE 语法 (无反斜杠)
- **Lesson**: 任何 grep 脚本都必须跑**正反两路径** (clean + drift) 自验证

### 2.3 终端 output truncation
- 多 command 串在 `for` 循环里,长 output 被截断
- 解决: 拆成多次单独 RunCommand
- **Lesson**: complex bash pipeline 输出要分批

---

## 3. 测量 vs 预期

| 指标 | 预期 | 实际 |
|------|------|------|
| 修复 spec 数 | 12 | 12 ✓ |
| Pattern A 修复时间 | <2 min | <1 min (5 sed) ✓ |
| Pattern B 修复时间 | <5 min | <3 min (7 prepend + 7 sed) ✓ |
| CI script 行数 | <30 | ~55 (含注释) ≈ |
| openspec validate (本 change) | PASS | PASS ✓ |
| 0 source code changes | 0 | 0 ✓ |
| 4-party 审查耗时 | <10 min | ~5 min ✓ |
| 实施 + 验证总耗时 | <15 min | <10 min ✓ |

---

## 4. 对项目 memory 的更新

### 4.1 New lessons
- **openspec CLI 1.3.1 数字开头 change name 限制**: `list`/`validate` 接受, `status`/`instructions apply` 拒绝. 影响未来 openspec-apply-change skill usage
- **bash regex ERE 语法**: `grep -E` 不需要 backslash-paren 和 backslash-pipe. 与 BRE (`grep` 默认) 不同
- **CI script 自验证原则**: 任何 lint/check 脚本必须跑正反两路径 (clean + drift) 才算完成

### 4.2 Updated patterns
- **OpenSpec cumulative format**: 全仓库 61/61 specs 已在 cumulative format (was 49/61)
- **CI gate pattern 复用**: `scripts/check_synced_spec_format.sh` 加入 `scripts/check_*.sh` family (next to `check_reexports.sh`)
- **4-party review ROI**: 简单任务 (12 文本修复) 仍能通过 4-party review 发现 6 项 design bug

### 4.3 Spec gotchas confirmed
- **change spec.md h1 vs h2**: `# ADDED Requirements` 是 markdown title,不被 parser 识别为 delta. 必须是 `## ADDED Requirements` (h2)
- **delta spec first sentence**: 必须含 SHALL 或 MUST (openspec 1.3.1 验证规则)

---

## 5. Follow-ups (out of scope for this change)

### F1: openspec CLI bug
- **Issue**: 数字开头 change name 在 `status`/`instructions apply` 命令中拒绝
- **Upstream**: openspec 1.3.1
- **Workaround**: 手动按 tasks.md 实施 (本 change 已用)
- **Long-term**: 升级 openspec 或 fork patch

### F2: pre-archive hook
- **Issue**: archive change 时容易忘记 strip delta headers
- **Mitigation idea**: git hook 在 `openspec archive` 前 grep `openspec/changes/<name>/specs/*/spec.md` 是否有 `## ADDED/MODIFIED`,若有警告
- **Priority**: low (本 change 已修 + CI gate 已防)

### F3: openspec-spec-format 跨项目 linter
- **Idea**: 抽 `openspec-spec-format` 为独立 crate 或 sub-tool
- **Blocker**: YAGNI, bash script 20 行足够
- **Priority**: very low

---

## 6. Process improvements (for next OpenSpec fix-style change)

1. **Before drafting design**: 先跑 4-party 审查 (发现 6 项 bug 是高 ROI)
2. **Pattern 分类**: 修复任务前先按 "kind of fix" 分类, 实施时按类批量
3. **CI script 自验证**: 写完必须跑 clean + drift 两路径
4. **E2E verification command**: 准备一段能粘到 verify.md 的可重现 command
5. **openspec tool 限制**: 提前记录工具不一致点,避免阻塞

---

## 7. Final score

- **Functionality**: 12/12 specs pass strict validate ✓
- **Code quality**: 0 source changes, 0 Cargo.toml changes, ~50 line bash script ✓
- **Documentation**: 8 OpenSpec artifacts (proposal/design/tasks/spec/plan/brainstorm/verify/retrospective) ✓
- **Process**: 4-party review + Socratic deconstruction + classification pattern + CI gate ✓
- **Risk mitigation**: 6 design bugs caught before implementation, 1 impl bug caught by self-test ✓

**Overall**: Strong success. 4-party review + Socratic deconstruction was high-value, even for "simple" spec format fixes.
