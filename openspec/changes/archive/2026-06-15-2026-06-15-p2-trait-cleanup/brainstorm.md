# Brainstorm: p2-trait-cleanup

> 2026-06-15 — 4-party 对抗性审查 + Socratic 问题分解
> 输入: trait-abstraction-review 的 12 个 P2 候选 + 重新 pre-flight 审计

## 4-party 审查 (12 候选)

| 候选 | 怀疑派 | 简化派 | 架构派 | 生产派 | 共识 |
|------|--------|--------|--------|--------|------|
| DoomLoopHandler | REMOVE | REMOVE | REMOVE | REMOVE | 4-0 |
| AuditWriter | REMOVE | REMOVE | REMOVE | REMOVE | 4-0 |
| EventStream | REMOVE | REMOVE | REMOVE | REMOVE | 4-0 |
| SkillMatcher | REMOVE | REMOVE | REMOVE | REMOVE | 4-0 |
| McpClient (mcp_bridge) | REMOVE-MODULE | REMOVE-MODULE | REMOVE-MODULE | REMOVE-MODULE | 4-0 (模块孤儿) |
| RiskEvaluator | REMOVE-DYN | REMOVE-DYN | KEEP-DYN | REMOVE-DYN | 3-1 |
| AuditLogger | REMOVE-DYN | REMOVE-DYN | KEEP-DYN | REMOVE-DYN | 3-1 |
| ContextService | REMOVE-DYN | REMOVE-DYN | KEEP-DYN | REMOVE-DYN | 3-1 |
| SessionWriter | REMOVE | REMOVE | KEEP | REMOVE | 3-1 (NoOp 唯一 impl) |
| PersistenceService | REMOVE-TRAIT | REMOVE-TRAIT | KEEP | REMOVE-TRAIT | 3-1 |
| ShellExecutor (mod.rs) | REMOVE | REMOVE | REMOVE | REMOVE | 4-0 |
| ShellExecutor (README) | CLEAN | CLEAN | CLEAN | CLEAN | 4-0 |

**结论**: 12/12 ≥ 3-1 共识,11/12 = 4-0。

## Socratic 问题分解

### Q1: "1 impl + 0 dyn = YAGNI" 的边界在哪?
**A**: 真实场景区分 3 种:
- (a) 真 YAGNI: 0 impl,0 调用 → 完全孤儿
- (b) 软 YAGNI: 1 impl,0 dyn,bounded test 验证抽象存在 → trait 仅用于类型断言
- (c) 预留 YAGNI: 1 impl,0 dyn,但 trait 暴露公共 API (with_*) → 抽象有"未来扩展"潜力

本次 12 候选中:
- DoomLoopHandler, AuditWriter, EventStream, SkillMatcher, ShellExecutor (mod.rs) → (b) 类
- McpClient → 整个模块孤儿 (a) 类
- RiskEvaluator, AuditLogger, ContextService, SessionWriter → (c) 类 (有公共 with_* 构造方法)
- PersistenceService → (c) 类 (但用 trait 作为内部 UFCS namespace)

**裁决**: (a)(b) 直接删除;(c) 需要权衡 — 4 派已投票,3-1 共识 REMOVE。

### Q2: dyn 调度的"成本"是否被高估?
**A**: 真实场景区分:
- `Arc<dyn Trait>` 字段: 每次访问 1 次虚表查找,~5ns;heap allocation,~50ns
- 抽象"灵活性"收益: 0 (无第二个 impl)
- **结论**: dyn 开销 > 收益,移除合理

### Q3: 删除 mcp_bridge 模块的影响?
**A**: 验证:
- `pub mod mcp_bridge` 在 `lib.rs:26`
- `grep -rn 'mcp_bridge' crates/ --include='*.rs'` → 0 外部引用 (除自身)
- `McpBridgeClient::call_tool` 返回 "not implemented" (非功能实现)
- `synthia-mcp/src/mcp_tool.rs` 存在**同名 `McpTool`** 但用 `McpManager` 而非 `McpClient` — 不冲突

**裁决**: 整个模块是孤儿,可安全删除。

### Q4: PersistenceService 7 方法怎么"inherent"到 Store?
**A**: 验证:
- `Store` 已存在 (from `synthia-session/src/store.rs`)
- 7 方法 UFCS 调用均在 `service.rs` 的 tests 中
- 13 个调用点可以 1:1 改为 `store.method(...)`
- 公共 API 破坏: `synthia_session::PersistenceService` 不再可导入
- 影响 reexport_policy.rs 测试

**裁决**: 标准 inherent 转换,无意外风险。

### Q5: with_risk_evaluator 改为具体类型后,是否破坏未来扩展?
**A**: 生产派 vs 架构派分歧:
- 架构派: 留 generic 形参,允许将来 mock
- 生产派: 留 generic 是空想 — 6 个月内无 mock 需求,且 mock 仍可通过 `StandardRiskEvaluator` 的内部 trait 模拟
- 怀疑派: generic 形参 = "可能用" 的掩饰,本质是决策瘫痪

**裁决**: 3-1 REMOVE-dyn,具体类型方法名 `with_standard_risk_evaluator`(明确意图)。

### Q6: README 重复定义的"价值"?
**A**: README 重复 `pub trait ShellExecutor` 块:
- "价值": 让读者不用跳转 mod.rs 即可看到 trait
- "成本**: grep 污染 (双重定义),未来修改需同步 2 处
- **裁决**: 4-0 删除 README 重复,改用文字描述 + 链接

### Q7: 12 候选合并 1 change 还是 12 change?
**A**: 候选特征:
- 都是 YAGNI trait 移除 (同质)
- 都是 P2 优先级 (同优先级)
- 1 OpenSpec change with 12 sub-tasks 比 12 changes 更易管理
- 沿用 P0 模式 (1 change 3 sub-tasks A/B/C)

**裁决**: 1 change,4 sub-tasks by complexity tier,12 commits per trait。

## 与 P0/P1 决策对齐

| 维度 | P0 (trait-review-remediation) | P1 (skillprovider) | P2 (本 change) |
|------|-------------------------------|--------------------|----------------|
| 范围 | 3 traits (Retryable, McpClientFacade×2, SessionManager) | 1 trait (SkillProvider) | 12 traits |
| 共识要求 | 4-0 | 4-0 (重审后) | 3-1 minimum |
| Commit 模式 | 1 commit per trait/concern | 1 commit | 1 commit per trait |
| 公共 API 破坏 | 透明记录 | 透明记录 | 透明记录 |
| Spec 格式 | ADDED Requirements | ADDED Requirements | ADDED Requirements |

## 边界

- **不做**: 任何性能优化、metrics、bug 修复、6 个 KEEP-dead? trait 调查
- **依赖**: 复用 P0/P1 已建立的 `openspec validate` 工作流 + `check_synced_spec_format.sh` CI
- **后续**: P2 完成后,下一段可选 (a) KEEP-dead 调查 (b) 新主题 (非 trait)
