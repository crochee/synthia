## Context

Synthia 是模块化 Rust AI Agent 框架（21 个 crate），实现 ReAct 模式。当前存在两套并行架构（legacy Agent + StreamBuilder），已完成第一轮清理（删除 executor/ + builder/，~681 行死代码）。

对比 OpenCode（TypeScript）和 Codex（Rust），Synthia 在三个方面存在差距：(1) OpenCode 支持 Markdown 文件定义 Agent，Synthia 硬编码；(2) Synthia 缺乏三层权限覆盖机制；(3) Synthia 缺乏多 Agent 控制平面（层级树、Mailbox、AgentPath）。

三位专家（architect、security、multiagent）已完成对抗性设计评审，11 个决策点三方共识，三份 v3 设计文档定稿。

## Goals / Non-Goals

**Goals:**
1. 文件式 Agent 定义 — 用户可通过 Markdown 文件自定义 Agent
2. 多层权限合并 — 支持 defaults → agent → user 三层覆盖
3. 多 Agent 控制平面 — 支持层级树形结构、Agent 间通信、资源管理
4. 保持向后兼容 — 现有 API 全部保持
5. 与 StreamBuilder 集成 — 新功能基于新架构

**Non-Goals:**
1. 不迁移 CLI 到 run_stream（后续单独处理）
2. 不统一三处权限系统（P1 follow-up）
3. 不实现远程 Agent 通信
4. 不改变现有单 Agent 行为

## Decisions

### D-1: pattern 语法
- **选择**: multi-segment colon glob (`bash:rm*` / `fs:write:/etc/**`)
- **理由**: 与 OpenCode 语义对齐，支持工具族 + 路径/参数细粒度策略
- **已考虑 alternatives**: OpenCode 原生 `tool:subcommand` 形式 — 拒绝，因为不够表达路径级别权限

### D-2: PermissionMode 处理
- **选择**: 删除 PermissionMode，改用 `Option<PermissionAction>` + None 哨兵
- **理由**: 简化类型系统，YAML 反序列化器将 "inherit" 映射为 None
- **已考虑 alternatives**: 保留 4 态 enum 做简化语法糖 — 拒绝，增加类型复杂度

### D-3: extends 权限合并语义
- **选择**: 规则级合并 + child priority（子规则按 pattern 去重覆盖父规则）
- **理由**: 最小权限原则，与 ForkPolicy 语义一致
- **已考虑 alternatives**: 全对象替换（父 permission 完全丢弃）— 拒绝，保留父规则更安全

### D-4: tools/denied_tools 组合
- **选择**: `allowed_tools` 预过滤（ToolRegistry 阶段）+ `denied_tools` 强制 Deny（MergedPolicy 之后叠加）
- **理由**: 职责分离，allowed_tools 早于权限检查，denied_tools 补充最终黑名单
- **已考虑 alternatives**: 二者都走 MergedPolicy — 拒绝，denied_tools 需要强制效力

### D-5: MergedPolicy crate 归属
- **选择**: `synthia-permission`
- **理由**: 与 PermissionRule 同 crate，synthia-agent 仅做加载，synthia-core 保持核心纯净
- **已考虑 alternatives**: synthia-core — 拒绝，跨 crate 抽象泄漏

### D-6: Ask 阻塞时 mailbox 行为
- **选择**: parent mailbox 强制 Suspended，用户回复后自动切回 NextTurn
- **理由**: 防止子 Agent 消息污染当前 turn，等待用户确认期间累积消息
- **已考虑 alternatives**: CurrentTurn 继续处理 — 拒绝，Ask 阻塞语义要求暂停

### D1: extends 与 AgentPath.parent 解耦
- **选择**: 完全独立（extends 是加载时 frontmatter 合并，AgentPath.parent 是运行时 spawn 层级）
- **理由**: 数据流完全不交叉，无实现耦合
- **已考虑 alternatives**: extends 链影响 parent — 拒绝，职责分离更清晰

### D2: 默认 Fork 组合
- **选择**: `SystemOnly + InheritAsUser`
- **理由**: SystemOnly 防止父 LLM 上下文污染子 Agent；InheritAsUser 隔离父的 User 层决策
- **已考虑 alternatives**: InheritAll — 拒绝，危险（父的 User deny 会传递）

### D3/D7: id 命名约束
- **选择**: `[a-z0-9][a-z0-9_-]{0,63}`（首字符必须字母或数字）
- **理由**: 与 POSIX 文件名 / DNS label 规范兼容，防止 `AgentPath::child("-root")` 边界 bug
- **已考虑 alternatives**: `[a-z0-9_-]{1,64}` — 拒绝，宽松导致边界情况

### D4: 消息 injection_scan
- **选择**: v1 仅工具级 gate，v2 叠加 Guardian injection_scan
- **理由**: v1 不引入 Guardian 耦合，避免控制平面与安全平面循环依赖
- **已考虑 alternatives**: v1 直接加 injection_scan — 拒绝，耦合过紧

### D5: 三处权限系统统一
- **选择**: P1 follow-up 单独 PR
- **理由**: 复杂度高，本 PR 仅升级 synthia-permission
- **已考虑 alternatives**: 本 PR 同时统一三处 — 拒绝，风险过高

## Risks / Trade-offs

[Risk] 三处权限系统统一复杂度高
→ Mitigation: P1 follow-up 单独 PR，本 PR 仅升级 synthia-permission

[Risk] 热重载与运行实例一致性问题
→ Mitigation: sub-agent 持有 definition clone + content_hash，不受后续热重载影响

[Risk] Mailbox prompt injection
→ Mitigation: v1 仅工具级 gate，v2 叠加 Guardian injection_scan（不阻断，仅审计）

[Trade-off] 文件式 Agent灵活性 vs 配置复杂度
→ 接受理由: YAML frontmatter 已是行业标准，用户已熟悉

[Trade-off] 多 Agent 控制平面引入复杂性
→ 接受理由: 资源管理（原子计数、nickname 池、深度限制）防止滥用

## Migration Plan

**阶段1: P0 文件式 Agent 定义 (5.5 人天)**
1. frontmatter 解析 + 类型定义
2. AgentFileLoader 扫描 + 校验
3. extends 继承合并逻辑
4. notify watcher + debounce 热重载
5. 与 AgentDefinition 桥接

**阶段 2: P0 多层权限合并 (5.5 人天)**
1. PermissionRule 数据结构
2. MergedPolicy 三层合并算法
3. pattern 匹配器
4. AskNotifier trait + Guardian 集成
5. 向后兼容适配器

**阶段 3: P1 多 Agent 控制平面 Phase 1 (5-7 人天)**
1. AgentPath + AgentRegistry
2. AgentControl 控制句柄
3. Mailbox 基本通信
4. StepSpawn 集成

**部署验证**: 每阶段完成后运行 `cargo test -p synthia-agent --lib`

**Rollback**: Git revert 阶段 commit，不影响其他阶段

## Open Questions

无 — 所有 11 个决策点已三方确认。

遗留 P1 follow-up 项:
- 三处权限系统统一（synthia-guardian + synthia-tool）
- CLI 迁移到 run_stream