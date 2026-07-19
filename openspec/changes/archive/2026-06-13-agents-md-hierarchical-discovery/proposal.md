## Why

Synthia 的 agent 在执行任务时缺乏项目级指令注入能力，无法从项目根目录的
`AGENTS.md` 读取约定（如代码风格、测试要求、禁止事项）。当前
`IdentitySection` 已有部分 `AGENTS.md` 处理但仅读取 `workspace_dir` 直接
子文件（无层级遍历），且与其他 3 个文件（`IDENTITY.md / USER.md /
MEMORY.md`）混在同一 section 注入；缓存级别 `Cached`（全局静态）导致文件
修改后静默 stale。

业界惯例（opencode / codex / Aider）都通过层级遍历 + 显式 section 注入来
解决此问题，Synthia 需要对齐。本次 change 把 `AGENTS.md` 提升为独立 section，
并实现 cwd → fs root 的层级发现与合并。

## What Changes

**AGENTS.md Discovery & Injection**
- From: `IdentitySection` 内联 `AGENTS.md`（仅 `workspace_dir` 直接子文件，
  `Cached` 全局静态）
- To: 独立 `AgentsMdSection`，从 `workspace_dir` 沿 `ancestors()` 向上遍历
  到 fs root，按"远→近"顺序拼接所有 `AGENTS.md`，缓存级别 `SessionCached`
- Reason: monorepo 项目需要在子目录工作时有全局 AGENTS.md 可读；当前
  Cached 缓存导致用户修改文件后看不到效果
- Impact: 内部行为变化；system prompt 长度增加（典型项目 < 10K chars）；
  关闭开关可恢复原行为

**IdentitySection Workspace Files**
- From: `WORKSPACE_FILES = ["AGENTS.md", "IDENTITY.md", "USER.md", "MEMORY.md"]`
- To: `WORKSPACE_FILES = ["IDENTITY.md", "USER.md", "MEMORY.md"]`
- Reason: AGENTS.md 改由 `AgentsMdSection` 处理，语义与缓存策略分离
- Impact: AGENTS.md 不再出现在 Identity section；可在独立 section 看到

**AgentConfig Field Additions**
- From: 无 `agents_md_*` 字段
- To: `agents_md_enabled: bool`（默认 `true`）+ `agents_md_filenames: Vec<String>`
  （默认 `["AGENTS.md"]`）
- Reason: power-user 可关闭或扩展文件名
- Impact: 新增两个字段；现有调用站点使用 `..Default::default()` 模式兼容

## Capabilities

### New Capabilities
- `agents-md-hierarchical-discovery`: 层级发现 + 合并 + 注入 system prompt
  的完整 capability，含可配置性、缓存、大小限制、错误处理

### Modified Capabilities
- `convergent-prompt-assembly`: IdentitySection 的 `WORKSPACE_FILES` 常量
  移除 `AGENTS.md`（AGENTS.md 注入位置变化属于行为变更）

## Impact

- **代码**：
  - 新增 `crates/synthia-context/src/prompt/sections/agents_md.rs`
  - 修改 `crates/synthia-context/src/prompt/sections/mod.rs`（导出新 section）
  - 修改 `crates/synthia-context/src/prompt/sections/identity.rs`（移除
    `WORKSPACE_FILES` 中的 `"AGENTS.md"`）
  - 修改 `crates/synthia-context/src/prompt/builder.rs`（`default_with_sections`
    和 `build_for_name` 接入新 section）
  - 修改 `crates/synthia-agent/src/config/agent_config.rs`（新增 2 字段，
    `Default` impl 同步）
  - 修改所有 `AgentRunConfigBuilder` 调用点（参考已有 `..Default::default()`
    模式；grep 显示 5 处：agent.rs / server/state.rs / server/routes/ws.rs /
    server/routes/chat.rs / cli/repl_core/repl.rs）
- **测试**：新增 `agents_md.rs` 单元测试（无文件/单文件/多层/超限截断/禁用），
  集成测试 `crates/synthia-context/tests/agents_md_discovery.rs`
- **依赖**：无新增 crate
- **风险**：AGENTS.md 增大 system prompt（受 60K 总和上限保护）
- **回滚**：每 PR 可独立 revert；IdentitySection 的 AGENTS.md 移除是单向迁移
