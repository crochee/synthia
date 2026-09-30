# 改了什么 & 为什么

<!--
PR 标题用 Conventional Commits: `feat(scope): summary` / `fix(scope): summary` /
`docs:` / `refactor:` / `test:` / `chore:` / `perf:` /
`ci:`。本模板做正文。
-->

### 背景

<!-- 一句话: 这条 PR 解的是什么问题 / 加的什么能力。 -->

### 改动

<!-- 文件 + 关键符号级别; 不要写整段代码. -->

- `crates/<x>/src/<file>.rs`: <关键符号 / 函数 / 类型>
- `crates/<x>/README.md`: <文档要点>
- ...

### 风险

<!-- 公共 API 破坏性? 性能影响? 安全相关? 模型 wire type 变更? -->

### 验证

<!-- 不是 "CI 绿"; 是行为级: -->

- [ ] `make ci` 本地绿
- [ ] `make examples` 绿（适用时）
- [ ] 端到端验证（命令 + 实际输出）：<!-- todo: -->
- [ ] 边界 / 失败处理有测试覆盖（适用时）
- [ ] 公开 API 变更 → 同步更新 `MINIMAL.md` / `SEAMS.md` / `CHANGELOG.md` 的 `[Unreleased]` 段

### 关联

- Closes #<n>
- Refs #<n>
- 关联 spec：`docs/<...>.md`，第 X 节

## Checklist

<!-- Maintainer review 锚点; 不要勾空话. -->

- [ ] DCO：`git commit -s`（见 [CONTRIBUTING.md](../CONTRIBUTING.md)）
- [ ] Conventional Commits 标题
- [ ] 没有未解的 TODO / FIXME / 占位实现（"Scaffold" / "MVP" / "v1" /
      "follow-up" 都是反模式 —— 见 `AGENTS.md` <completeness>）
- [ ] 没有引入 `pub use <非 synthia_*>::*;`（`make check-pub-surface`）
- [ ] 没有引入运行时 crate 别名泄漏（`make check-public-api-runtime`）
- [ ] 没有引入 `chrono::Utc::now()` 到生产代码（`make check-clock`）
- [ ] 离线 tool crate 仍然零 HTTP 依赖（`make check-mvp-deps`）
- [ ] 没有可测试性 / 不变量 / 形状退化（`make check-harness-shape` /
      `make check-test-layout` / `make ci`）

## 接口契约同步（双侧闭合）

<!-- 若改 synthia-server 路由或 synthia-web api 调用时填；其它 PR 删本段。 -->

- [ ] `make contract-scan` 已跑（产出 `docs/interface-contract/contract.{md,yaml,json}`）
- [ ] `make contract-check` 全绿（exit 0，无 frontend-only endpoint）
- [ ] 改了契约 → `openspec/specs/<capability>/spec.md` 按 ADDED/MODIFIED 流程补 Requirement
- [ ] commit message 或本描述里写明仲裁源（`Synthia chat wire contract` /
      `Synthia stable spec §X.Y`）

## 关联

- Fix card(s) #:
- Spec(s) updated:
- Breaking change: 是 / 否