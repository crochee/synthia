## Context

Synthia 是一个 AI agent 框架，需要让 agent 在执行任务时了解项目特定的约束、
约定和工作流。业界惯例（opencode / codex / Aider / Claude Code）通过在工作
目录及其祖先路径上查找 `AGENTS.md` 文件来实现项目级指令注入。

当前 Synthia 状态：
- `IdentitySection` 中有部分 `AGENTS.md` 处理（`crates/synthia-context/src/prompt/sections/identity.rs:23-24`）
- 但只读取 `workspace_dir` 下的直接子文件（无层级遍历）
- 与 `IDENTITY.md / USER.md / MEMORY.md` 混在同一个 section 注入
- 缓存级别为 `Cached`（全局静态）→ 静默 stale 风险

本次 change 目标：实现 opencode/codex 同等的 AGENTS.md 层级发现能力，作为
独立的 prompt section，缓存策略正确，可配置。

## Goals / Non-Goals

**Goals:**
- 实现 `AGENTS.md` 从 `workspace_dir` 向上到 fs root 的层级遍历
- 合并顺序：近→远（具体规则可 override 全局规则）
- 注入到 system prompt 的固定位置，独立 section，可观测
- 缓存策略 `SessionCached`（session 内复用，session 间重读）
- 可配置：禁用 / 改文件名 / 调大小限制
- 行为可测试、可观测（有日志、可注入追踪）

**Non-Goals:**
- 不支持 `CLAUDE.md / GEMINI.md / CODEX.md` 等其他命名（除非用户主动加
  进 `filenames` 配置）
- 不做 monorepo 专用 `package.json#workspaces` 解析
- 不做 file-watcher 实时刷新（已有 OpenSpec change `config-watcher`）
- 不做 AGENTS.md 的 LLM 摘要压缩
- 不修改 IdentitySection 中其他 3 个文件（`IDENTITY.md / USER.md /
  MEMORY.md`）的注入位置（但会从 `WORKSPACE_FILES` 常量中移除 `AGENTS.md`）

## Decisions

### D1：独立 `AgentsMdSection`，从 IdentitySection 移除 AGENTS.md

- **选择**：新建 `synthia_context::prompt::sections::agents_md::AgentsMdSection`；
  从 `IdentitySection::WORKSPACE_FILES` 常量中移除 `"AGENTS.md"`
- **理由**：Identity 描述 agent 自身身份；AGENTS.md 描述项目约定。语义不同，
  缓存策略不同（Identity 是 `Cached`，AGENTS.md 是 `SessionCached`），
  观测粒度不同
- **已考虑 alternative**：保留 Identity 内联 → 拒绝，因为
  - Identity 是 Cached（全局静态），AGENTS.md 需要感知文件变化
  - 混在一起无法独立测、独立配置

### D2：合并顺序：近→远

- **选择**：从 `workspace_dir` 沿 `Path::ancestors()` 向上，每个祖先目录
  找到 `AGENTS.md` 后**先压栈**，最后弹栈拼接（即"远→近"顺序输出，但
  遍历方向是"近→远"）
- **理由**：opencode 语义是"最具体的覆盖全局"；LLM 处理时把具体规则放
  在最后更符合 CSS cascade 认知
- **已考虑 alternative**：远→近（全局在前） → 拒绝，违反 override 直觉

### D3：缓存级别 `SessionCached`

- **选择**：`SectionCaching::SessionCached`
- **理由**：
  - AGENTS.md 可能在 session 中途被修改（用户 `vi AGENTS.md` → 重新 build
    应反映新内容）
  - 同一 session 内多次 LLM 调用间应复用（避免重复 IO）
  - 不同 session 必须重读（文件可能跨 session 改变）
- **已考虑 alternative**：`Cached`（全局静态） → 拒绝，是当前 bug；
  `Volatile`（每次重读） → 拒绝，浪费 IO；`Uncached`（不缓存） →
  拒绝，与 SessionCached 等价但语义不清

### D4：大小限制

- **选择**：
  - 单文件 `MAX_CHARS_PER_FILE = 20_000`（与 `inject_workspace_file` 一致）
  - 总和 `MAX_CHARS_TOTAL = 60_000`（约 15K tokens，符合 context budget
    预留）
  - 超出截断并附加 `[... truncated ...]` marker
- **理由**：防止恶意/超大文件抢占 context；总和限制保护 budget
- **已考虑 alternative**：不限制 → 拒绝，恶意项目可注入巨大内容；
  100K 限制 → 拒绝，挤压太多

### D5：文件命名配置

- **选择**：`AgentConfig` 新增 `agents_md_filenames: Vec<String>`，默认
  `vec!["AGENTS.md".to_string()]`
- **理由**：用户可能想加 `CLAUDE.md` 别名；不强制硬编码
- **已考虑 alternative**：硬编码 `AGENTS.md` → 拒绝，违反 6-month 模式
  预留扩展点

### D6：禁用开关

- **选择**：`AgentConfig.agents_md_enabled: bool`，默认 `true`
- **理由**：业界默认开启；power-user 可关闭
- **已考虑 alternative**：opt-in → 拒绝，违反 opencode/codex 默认行为

### D7：路径遍历终止条件

- **选择**：循环 `Path::ancestors()`，每个祖先 `canonicalize()` 后检查
  - 已访问集合防 symlink 环
  - 到达 fs root（`parent()` 返回 `None`）停止
  - 跳过 `.git` 目录（视作 boundary）
- **理由**：防御 symlink 攻击 / 跨盘逃逸；`.git` 是 git 仓库 root 哨兵
- **已考虑 alternative**：仅在 `Path::ancestors()` 上无脑循环 → 拒绝，
  容易因 symlink 死循环

### D8：错误处理

- **选择**：
  - 文件不存在或 `is_dir()` → 跳过（正常情况）
  - 文件不可读（权限） → `tracing::warn!` + 跳过
  - 单文件超过 `MAX_CHARS_PER_FILE` → 截断 + marker
  - 总和超过 `MAX_CHARS_TOTAL` → 截断 + marker
  - 无任何 AGENTS.md 找到 → 返回空字符串（让 `PromptBuilder` 跳过该 section）
- **理由**：单点失败不应阻塞整个 section；可观测性优先

### D9：日志

- **选择**：
  - `tracing::debug!(path, size_chars, "AGENTS.md loaded")`
  - 加载失败 `tracing::warn!(path, error, "AGENTS.md load failed")`
- **理由**：可观测性（P9）要求路径可追溯；用户可启用 debug 日志确认
  哪些文件被加载

### D10：配置字段命名

- **选择**：
  - `AgentConfig::agents_md_enabled: bool`
  - `AgentConfig::agents_md_filenames: Vec<String>`
- **理由**：与 `context_token_budget` / `checkpoint_dir` 等命名风格一致
  （snake_case + 类型后缀）

## Risks / Trade-offs

- **[Risk] AGENTS.md 注入挤占真实对话 token** → Mitigation: 60K 总和上限
  + 截断 marker；用户可关闭
- **[Risk] 用户误把 secret 写进 AGENTS.md 然后泄露给 LLM** → Mitigation:
  这是用户责任；可加 warning 日志（v2 跟进，本 change 不做）
- **[Risk] 路径遍历性能开销** → Mitigation: 深度受限（典型 5 层），
  小文件 IO（< 20K），单次 build 只读一次
- **[Risk] symlink 死循环** → Mitigation: canonicalize + visited set
- **[Trade-off] `SessionCached` 缓存可能 stale（同一 session 内文件被
  修改）** → 接受理由：用户主动修改 AGENTS.md 后期待"下次启动 session
  生效"是更自然的语义；session 内主动修改的场景罕见

## Migration Plan

本 change 不涉及部署变更（纯加 crate 内部模块 + AgentConfig 字段）：

1. PR 1: 加 `AgentsMdSection` 模块 + 单元测试（不挂到 PromptBuilder）
2. PR 2: 接入 `PromptBuilder::default_with_sections()` 和 `build_for_name()`；
   从 `IdentitySection::WORKSPACE_FILES` 移除 `"AGENTS.md"`
3. PR 3: 在 `AgentConfig` 加 `agents_md_enabled` / `agents_md_filenames` 字段
   + 同步所有 builder 路径（参考已有 `..Default::default()` 模式）

**回滚策略**：每个 PR 可独立 revert；IdentitySection 的 AGENTS.md 移除
是单向迁移（无兼容需求，因为 `Cached` 注入本来就有 stale bug）

## Open Questions

无。已通过 brainstorm 完成 12 个关键决策。
