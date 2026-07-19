<!--
Raw capture of design decision log for AGENTS.md hierarchical discovery.

本文档为设计决策的 raw capture 记录。design.md 后续会从这里抽取并重组。
-->

# Brainstorm: AGENTS.md Hierarchical Discovery

## Background (from gap assessment 2026-06-13)

opencode, codex, Aider, Claude Code 等业界 agent 都支持 `AGENTS.md`（或等价物
`CLAUDE.md` / `AGENTS.md`）作为项目级指令文件。Synthia 现有
`IdentitySection` 中已经有部分 `AGENTS.md` 支持，但只读取 `workspace_dir` 直接
子文件（无层级遍历），并且把 4 个文件 (`AGENTS.md` / `IDENTITY.md` / `USER.md` /
`MEMORY.md`) 混在一起注入，位置也是 Cached（KV cache 命中目标，但实际上文件
修改无法感知 → 静默 stale）。

## Key Question 1: 要不要支持层级遍历？

**决策：必须支持**。

理由：单层读取无法处理 monorepo 场景（用户在 `crates/foo/` 工作，但根目录
的 `AGENTS.md` 才有全局约定）。opencode/codex 都走层级遍历，从 cwd 到 fs root
找到所有同名文件并按"近→远"或"远→近"合并。

## Key Question 2: 合并顺序：近→远 还是 远→近？

**决策：近→远（最具体的在末尾）**。

理由：opencode 的语义是"在文件系统中距离 cwd 最近的 AGENTS.md 拥有最终
发言权（override）"。LLM 在处理时自然会把"具体规则 > 全局规则"。这与
inherits / 配置覆盖的常规认知一致（CSS cascade 思维）。

## Key Question 3: 缓存策略？

**决策：`SessionCached`（dynamic）**。

理由：
- AGENTS.md 文件可能改变；同一 session 内修改文件 → 下次 build 应该反映
  新内容
- 但 LLM 调用之间（前缀稳定性）不需要每次重读文件 → SessionCached 已经
  满足"sessions 内复用 + session 间重读"
- 不该用 `Cached`（全局静态），否则用户改了文件但 prefix hash 不变 → 静默
  stale（这正是当前 IdentitySection 的 bug）

## Key Question 4: 文件名？

**决策：默认 `AGENTS.md`，不引入 `CLAUDE.md` 别名**。

理由：
- 业界默认是 AGENTS.md（Aider / opencode / codex）
- 引入多别名（CLAUDE.md / GEMINI.md / CODEX.md ...）超出 6-month-stabilization
  模式；YAGNI
- 但预留 `AgentsMdConfig { filenames: Vec<String> }` 字段，未来可加

## Key Question 5: 大小限制？

**决策：每文件 20_000 字符（与现有 `inject_workspace_file` 一致）；总数
也限 60_000 字符**。

理由：
- 单文件太大 → 抢占 KV cache，影响 prefix 稳定性
- 总数太大 → 突破 context budget
- 现有 `WORKSPACE_FILE_MAX_CHARS = 20_000` 是良好基线

## Key Question 6: 路径遍历到哪里停？

**决策：到 fs root（`Path::ancestors()` 终止条件）**。

但**加入"symlink 防环"**：`canonicalize()` 后若发现环或逃出原 dir
（path traversal），停止。

## Key Question 7: 与现有 IdentitySection 的 AGENTS.md 注入如何共存？

**决策：从 IdentitySection 中移除 AGENTS.md 注入，移到独立 section**。

理由：
- Identity 应该是 agent 自身身份的描述，不应混入项目级指令
- 独立 section 有独立的缓存、独立的可观测性
- IdentitySection 的 `WORKSPACE_FILES` 简化为 `IDENTITY.md / USER.md /
  MEMORY.md`（移除 `AGENTS.md`）

## Key Question 8: 配置开关？

**决策：默认开启（opt-out）**。通过 `AgentConfig.agents_md_enabled: bool`
和 `AgentConfig.agents_md_filenames: Vec<String>` 控制。

理由：业界默认就是开启；用户开启 Synthia 就期待"agent 懂项目"，关掉是
power-user 操作。

## Key Question 9: 空内容如何处理？

**决策：build 返回空字符串，让 PromptBuilder 跳过（已支持）**。

## Key Question 10: 文件不可读 / 编码错误？

**决策：log warning 跳过该文件，继续**。不要让单个坏文件 block 整个
section。

## Key Question 11: 性能影响？

- 每次 build 最多读 N 个文件（depth 通常 ≤ 5）
- 文件小（20K 上限），用 `std::fs::read_to_string` 即可
- 无需 async（避免传染），但要 quick

## Key Question 12: 风险 / 反对声音

- "AGENTS.md 注入会让 prompt 变长，挤压真实对话" → SessionCached 不影响
  KV cache；超出 60K 字符会截断；用户可用配置关闭
- "用户可能误把 secret 写进 AGENTS.md 然后泄露" → 这是用户责任；可加
  warning 日志"AGENTS.md loaded from <path>"
- "层级遍历慢" → 一次 build 只读一次，深度受限（5 层）

## Out of Scope

- 不支持 `CLAUDE.md / GEMINI.md / CODEX.md` 等其他命名（除非用户主动加
  进 `filenames` 配置）
- 不实现 monorepo 专用 `package.json#workspaces` 解析
- 不做 file-watcher 实时刷新（OpenSpec change `config-watcher` 已存在）
- 不做 AGENTS.md 的 LLM 摘要压缩
