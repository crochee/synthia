# Tasks: p0-trait-review-remediation

## Phase 1: Sub-task A (Retryable trait 删除)

- [ ] A.1 — 编辑 [crates/synthia-provider/src/retry.rs](file:///home/crochee/workspace/synthia/crates/synthia-provider/src/retry.rs), 删除 `pub trait Retryable` + `impl Retryable for Error` (lines 6-14)
- [ ] A.2 — 运行 `cargo check -p synthia-provider` 确认 0 errors
- [ ] A.3 — 运行 `cargo test -p synthia-provider` 确认 0 regressions
- [ ] A.4 — 运行 `grep -r 'Retryable' crates/` 确认 0 命中
- [ ] A.5 — 修正 [deep-reviews/06-Retryable.md](file:///home/crochee/workspace/synthia/openspec/changes/archive/2026-06-15-2026-06-15-trait-abstraction-review/artifacts/deep-reviews/06-Retryable.md) 的"潜在无限递归"描述 → "no-op wrapper, 非递归"
- [ ] A.6 — `git add crates/synthia-provider/src/retry.rs openspec/changes/archive/.../06-Retryable.md && git commit -m "p0-remediation A: remove dead Retryable trait"`

## Phase 2: Sub-task B (McpClientFacade dedup)

- [ ] B.1 — 编辑 [crates/synthia-mcp/src/types.rs](file:///home/crochee/workspace/synthia/crates/synthia-mcp/src/types.rs), 删除 `pub trait McpClientFacade` 块 (lines 94-105)
- [ ] B.2 — 编辑 [crates/synthia-mcp/src/traits.rs](file:///home/crochee/workspace/synthia/crates/synthia-mcp/src/traits.rs), 删除整个文件 (因为只含 trait + imports)
- [ ] B.3 — 运行 `cargo check -p synthia-mcp` 确认 0 errors
- [ ] B.4 — 运行 `cargo test -p synthia-mcp` 确认 0 regressions
- [ ] B.5 — 运行 `grep -r 'McpClientFacade' crates/` 确认 0 命中
- [ ] B.6 — 修正 [recommendations.md](file:///home/crochee/workspace/synthia/openspec/changes/archive/2026-06-15-2026-06-15-trait-abstraction-review/artifacts/recommendations.md) 中"编译错误"描述 → "语义重复 (模块内同名,不同签名)"
- [ ] B.7 — `git add crates/synthia-mcp/ && git commit -m "p0-remediation B: remove duplicate McpClientFacade trait definitions"`

## Phase 3: Sub-task C (SessionManager 拆分)

- [ ] C.1 — 探索: `grep -rn 'SessionManager' crates/ --include='*.rs'` 列出所有引用
- [ ] C.2 — 编辑 [crates/synthia-session/src/session.rs](file:///home/crochee/workspace/synthia/crates/synthia-session/src/session.rs), 删除 `pub trait SessionManager`
- [ ] C.3 — 同文件新增 `pub trait SessionReader` (6 方法 + 2 默认)
- [ ] C.4 — 同文件新增 `pub trait SessionWriter` (6 方法)
- [ ] C.5 — 找到 `impl SessionManager for Store`, 拆为 `impl SessionReader for Store` + `impl SessionWriter for Store`
- [ ] C.6 — 遍历 C.1 grep 结果, 更新每个 call site 的 trait bound:
  - 仅需读 → `R: SessionReader`
  - 仅需写 → `R: SessionWriter`
  - 都需 → `R: SessionReader + SessionWriter`
- [ ] C.7 — 运行 `cargo check --workspace` 确认 0 errors
- [ ] C.8 — 运行 `cargo test --workspace` 确认 0 regressions
- [ ] C.9 — 运行 `cargo clippy --all-targets --all-features --tests --all` 确认 0 warnings
- [ ] C.10 — 运行 `cargo +nightly fmt --all` 格式化
- [ ] C.11 — 运行 `grep -rn 'SessionManager' crates/ --include='*.rs'` 确认 0 命中 (除 commit message)
- [ ] C.12 — `git add crates/synthia-session/ && git commit -m "p0-remediation C: split SessionManager into SessionReader + SessionWriter"`

## Phase 4: 验证 + 归档

- [ ] V.1 — 运行 `cargo test --workspace` 确认全工作区 0 regressions
- [ ] V.2 — 运行 `cargo clippy --all-targets --all-features --tests --all` 确认 0 warnings
- [ ] V.3 — 运行 `cargo +nightly fmt --all --check` 确认格式正确
- [ ] V.4 — 填充 [verify.md](file:///home/crochee/workspace/synthia/openspec/changes/2026-06-15-p0-trait-review-remediation/verify.md) (7 阶段 + 3 sub-task 证据 + 4-party 共识记录)
- [ ] V.5 — 运行 `openspec validate 2026-06-15-p0-trait-review-remediation --strict` 确认 valid
- [ ] V.6 — 运行 `bash scripts/check_synced_spec_format.sh` 确认格式合规
- [ ] V.7 — 运行 `yes | openspec archive 2026-06-15-p0-trait-review-remediation` 归档

## 总计: 30 个 task

- Sub-task A: 6 task
- Sub-task B: 7 task
- Sub-task C: 12 task
- Phase 4 verify: 7 task

## 依赖关系

- A 完全独立, 可单独先做
- B 完全独立, 顺序在 A 之后为可读性
- C 独立, 顺序在 A/B 之后 (call site 涉及全工作区, 单独 commit 隔离风险)
- Phase 4 依赖 A+B+C 全部完成
