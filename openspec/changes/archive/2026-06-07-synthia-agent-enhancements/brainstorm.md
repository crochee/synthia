# Brainstorming Output: Synthia Agent Enhancements

## 三位专家对抗性设计评审 — 11 个决策点三方共识

---

## 1. 项目背景

Synthia 当前状态：
- 模块化 Rust AI Agent 框架，21 个 crate
- 新旧两套架构并行（legacy Agent + StreamBuilder）
- 已完成第一轮清理：删除 executor/ + builder/ (~681 行死代码)

差距分析（对比 OpenCode / Codex）：
1. **文件式 Agent 定义** — OpenCode 支持 Markdown + frontmatter 定义 Agent，Synthia 硬编码
2. **多层权限合并** — Synthia 缺乏三层覆盖（defaults → agent → user）
3. **多 Agent 控制平面** — Synthia 无层级树形结构、Mailbox 通信、AgentPath 寻址

---

## 2. 三个子项目

### 2.1 文件式 Agent 定义 (P0)
**设计文档**: `docs/superpowers/specs/2026-06-07-file-based-agent-design.md` (1607 行, v3)

**核心决策**:
- 目录结构: `.agents/agents/<id>.md` (新格式) + `.agents/agents/<id>/{metadata.yaml, SYSTEM.md}` (旧格式兼容)
- frontmatter 格式: YAML (vs TOML) — 多行字符串可读性更好
- `AgentDefinition` 增量扩展字段 (model/temperature/tools/permission/extends/mode 等)
- 新增 `AgentFileLoader` 独立模块
- **extends继承**: 子 agent 按 pattern 去重覆盖父规则 (child priority)
-加载时机: 启动同步扫描 + notify watcher + 500ms debounce 热重载
- 缓存: content_hash (SHA-256) 跳过未变更内容

**P0 决策 (7项)**:
- D-1: pattern 语法 → multi-segment colon glob (`fs:write:/etc/**`)
- D-2: 删除 PermissionMode → `Option<PermissionAction>` + None 哨兵
- D-3-extends: 规则级合并 + child priority
- D-3-id: id 命名约束 `[a-z0-9][a-z0-9_-]{0,63}`
- D-4: `allowed_tools` (预过滤) + `denied_tools` (强制 Deny)
- D-5: MergedPolicy 放 `synthia-permission`
- D-6: Ask 阻塞时 mailbox 强制 Suspended

**工作量**: 5.5 人天 (P0)

### 2.2 多层权限合并 (P0)
**设计文档**: `docs/superpowers/specs/2026-06-07-permission-merge-design.md` (940 行, v3.1)

**核心决策**:
- `PermissionRule { pattern, action: Allow|Deny|Ask, forced: bool }`
- 三层合并: `RuleLayer { Default=0, Agent=1, User=2 }`
- 评估: 多段冒号 glob 模式匹配 (`bash:rm*` / `fs:write:/etc/**`)
- Guardian 集成: 复用 `GuardianDecision::NeedUserConfirm` + `ToolAction::PendingConfirm`
- `forced: bool` 字段用于 `denied_tools` 叠加强制 Deny
- 向后兼容: 旧 TOML 配置继续工作

**P0 决策 (6项)**:
- D-1: pattern 语法 → multi-segment colon glob
- D-2: 删除 PermissionMode → `Option<PermissionAction>` + None
- D-3: 规则级合并 + child priority
- D-4: `denied_tools` 强制 Deny (`forced: true`)
- D-5: MergedPolicy 放 `synthia-permission`
- D-6: Ask 阻塞 → mailbox Suspended

**工作量**: 5.5 人天 (P0)

### 2.3 多 Agent 控制平面 (P1)
**设计文档**: `docs/superpowers/specs/2026-06-07-multi-agent-control-plane-design.md` (1754 行, v3.2 FINAL)

**核心决策**:
- **AgentPath** 层级寻址 (`/root/worker/explorer2`)
- **AgentRegistry** 统一注册表 (path→metadata + 原子计数 + nickname 池)
- **SpawnReservation** RAII 两阶段提交
- **Mailbox** + **MailboxDeliveryPhase** (CurrentTurn/NextTurn/Suspended 三态)
- **CompletionWatcher** detached tokio::spawn 监视
- **ForkPolicy** 5 种策略 + **ForkPermissionPolicy** 4 种策略
- 默认组合: `SystemOnly + InheritAsUser`

**P1 决策 (9项)**:
- D1: extends 与 AgentPath.parent 完全解耦
- D2: 默认 ForkPolicy::SystemOnly + ForkPermissionPolicy::InheritAsUser
- D3: 命名约束加严 `[a-z0-9][a-z0-9_-]{0,63}`
- D4: v1 工具级 gate，v2 加 injection_scan
- D5: P1 follow-up 统一 permission 系统
- D6: Ask 阻塞时 mailbox Suspended
- D7: id 命名约束 (同 D3-id)
- D8: definition_drift 仅 telemetry
- D9: parent 不显示 extends 链

**工作量**: 14-19 人天 (4 phases)

---

## 3. 实施顺序

```
P0 文件式 Agent 定义 (5.5d)
    ↓
P0 多层权限合并 (5.5d) — 依赖 P0 文件式 Agent
    ↓
P1 多 Agent 控制平面 (14-19d) — 依赖 P0两者
```

---

## 4. 关键技术细节

### 4.1 三处权限系统统一 (P1 follow-up)
- `synthia-permission` —升级为 MergedPolicy 主路径
- `synthia-guardian/permission.rs` — 标记 deprecated
- `synthia-tool/exec/permission.rs` — 标记 deprecated

### 4.2 向后兼容
-旧 `metadata.yaml` 加载路径继续工作
- 现有 `PermissionPolicy` API 不变
- CLI 的 legacy `Agent::new()` with `AgentDeps` 迁移后清理

### 4.3 与 StreamBuilder 集成
- `AgentRunConfig` 新增可选 `agent_control: Arc<AgentControl>` 字段
- 新增 `StepSpawn` 步骤拦截 `AgentTool` 调用
- `AgentEvent` 新增 4 个 Subagent 事件 variant

---

## 5. 决策登记 (11 项全部 confirmed)

| ID | 决策 | 结论 |
|---|---|---|
| D-1 | pattern 语法 | multi-segment colon glob |
| D-2 | PermissionMode | 删除 → Option<PermissionAction> + None |
| D-3 | extends 权限合并 | 规则级合并 + child priority |
| D-4 | tools/denied_tools | denied_tools 强制 Deny |
| D-5 | MergedPolicy 归属 | synthia-permission |
| D-6 | Ask 阻塞行为 | mailbox 强制 Suspended |
| D1 | extends vs AgentPath.parent | 完全解耦 |
| D2 | 默认 Fork 组合 | SystemOnly + InheritAsUser |
| D3/D7 | id 命名约束 | [a-z0-9][a-z0-9_-]{0,63} |
| D4 | 消息 injection_scan | v1 工具级 gate，v2 加 Guardian scan |
| D5 | 三处权限系统统一 | P1 follow-up |
| D6 | Ask 阻塞时 mailbox | 强制 Suspended |
| D8 | definition_drift | 仅 telemetry，不 cancel |
| D9 | extends 链显示 | parent 不显示 |

---

## 6. 风险与缓解

| 风险 | 级别 | 缓解 |
|---|---|---|
| 三系统权限统一复杂度 | 高 | P1 follow-up 单独 PR，本 PR 仅升级 synthia-permission |
| Mailbox 安全 (prompt injection) | 中 | v1 仅工具级 gate，v2 叠加 Guardian injection_scan |
| 热重载与运行实例一致性 | 中 | sub-agent 持有 definition clone + content_hash，不受后续热重载影响 |

---

## 7. 下一步

实施顺序：P0 文件式 Agent → P0 权限合并 → P1 控制平面