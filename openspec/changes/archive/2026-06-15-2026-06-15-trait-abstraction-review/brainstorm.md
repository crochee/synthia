# Brainstorm: trait-abstraction-review

> Session: 2026-06-15
> Method: Multi-perspective Socratic decomposition (4-party adversarial review: 怀疑派 / 架构派 / 生产派 / 简化派)

## 1. 触发问题

代码库中 57 个 `pub trait` 是否仍是最优抽象?6 个月前 (2025-12) 经历过
critical-bug + 重复代码修复后,部分 trait 可能:
- 仅 1 个 impl (YAGNI 嫌疑)
- 引入过重泛型/bounds (抽象过载)
- 仍存在但已无实际调用点 (dead abstraction)

## 2. 关键决策

### 2.1 范围: 全量 57 个, 不漏

| 选项 | 评估 |
|------|------|
| 全量 57 (研究产出物) | ✓ 选 - 全面 + 落子 (后续 refactor 索引) |
| 聚焦 1-2 个核心 | ✗ - 信息缺失,后续重做 |
| 仅 1-impl 嫌疑 | ✗ - 容易漏掉 multi-impl 但过载的 |

### 2.2 产出形式: OpenSpec change

| 选项 | 评估 |
|------|------|
| OpenSpec change (proposal+design) | ✓ 选 - 沿用项目现有 OpenSpec 习惯 |
| 独立 research report | ✗ - 不易追溯 |
| 仅 chat 输出 | ✗ - 不可审计 |

### 2.3 方法: Hybrid (A 全扫 + C 重点深掘)

| 选项 | 评估 |
|------|------|
| Heuristic 自动扫描 | 机械化 / 快, 但缺上下文 |
| 手动 code review | 质量高 / 慢, 57 个吃不消 |
| **Hybrid (A 全扫 + C 重点深掘)** | ✓ 选 - 平衡广度深度 |

## 3. 设计要点 (经 4 派审查)

- **怀疑派**: "1-impl 不一定坏, monorepo 限制下正常; 但若连调用点都少, 几乎肯定是过度抽象"
- **架构派**: "需要为 57 个 trait 维护一份'语义地图', 否则评审容易陷入局部"
- **生产派**: "评审不应引发重命名/路径变化, 风险大; 边界明确为 research only"
- **简化派**: "决策矩阵必须公开, 不能用'我看着办'; 评审标准量化"

## 4. 共识

- 7 阶段时间盒 ~2.5h
- 4-party 对抗性 review 写进 design.md
- 未来 refactor 索引段保留 (建议)
- 零新依赖 (rg + bash)
