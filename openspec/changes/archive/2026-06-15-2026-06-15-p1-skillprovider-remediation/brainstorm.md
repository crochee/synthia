# Brainstorm: p1-skillprovider-remediation

> 4 派对抗原始记录 (怀疑派 / 架构派 / 生产派 / 简化派)
> 编排: 2026-06-15 22:30 (Asia/Shanghai)
> 议题: trait-abstraction-review P1 `SkillProvider` 10 方法违反 ISP, 怎么收尾

## 第 0 步: 议题澄清 (Socratic 分解)

**Q1: Sub-task P0 SessionManager 的删除决策成功先例是否适用?**

- 怀疑派: **完全适用**。0 bound + 0 dyn + 1 impl = 同样的"纯预留"模式
- 架构派: 类似, 但 SkillProvider 涵盖 3 个独立关注点 (CRUD/匹配/向量), 拆分有意义
- 生产派: 类似, skill 系统确实活跃, 但 trait 抽象本身无用户
- 简化派: **完全适用**。0 实际用户 = YAGNI, 不管粒度多差

**用户决策**: 进入 Q2。

**Q2: 实际数据告诉我们什么?**

关键发现 (2026-06-15 重新审计):

```
$ grep -E ': SkillProvider|: &SkillProvider|dyn SkillProvider|Arc<SkillProvider>|Box<SkillProvider>' crates/
(0 matches)
```

| 信号 | 原始 inventory | 重新审计 (2026-06-15) |
|------|----------------|------------------------|
| impl | 1 | 1 ✓ |
| methods | 10 | 10 ✓ |
| call_sites (作为方法调用) | 0 | 0 ✓ |
| dyn | 0 | 0 ✓ |
| **trait bound usage** | (未统计) | **0** ⚠️ |
| **Arc/Box wrapping** | (未统计) | **0** ⚠️ |

`SkillProvider` 出现在 7 个文件, 但**全部是 6 处 `use` import + 1 处 `pub use` re-export**。没有：
- `impl Trait for SomeType` 之外的 trait bound (`T: SkillProvider`)
- `dyn SkillProvider` 虚分派
- `Arc<SkillProvider>` / `Box<SkillProvider>` 装箱
- `&SkillProvider` 参数传递

→ trait 是**纯结构性标签**: 唯一的"用户"是它自己的 impl 块。

**Q3: 与 SessionManager (P0 已删) 的对比**

| 维度 | SessionManager | SkillProvider |
|------|----------------|---------------|
| impl | 1 | 1 |
| methods | 12 | 10 |
| 0 trait bound | ✅ | ✅ |
| 0 dyn | ✅ | ✅ |
| 0 Arc/Box | ✅ | ✅ |
| 唯一"用户"是 impl 自己 | ✅ | ✅ |
| 之前决策 | **REMOVE** (4-0) | ? |

**结论**: SkillProvider 与 SessionManager 模式**完全一致**。SessionManager 已 2026-06-15 删除, SkillProvider 应同等处理。

## 第 1 派: 怀疑派 (Skeptic)

**核心立场**: "no user = no trait" — 任何 0 impl-bound + 0 dyn + 0 Arc/Box 包装的 trait 都应删除。

**对 SkillProvider**:
- ✅ 同意删除。0 trait bound 意味着 trait 在抽象层没有作用, 仅为 impl 提供类型标签。
- 10 个方法放在 impl block 里方法签名仍然清晰, Rust 文档注释 (`///`) 不依赖 trait。
- 拆分 3 个 trait (Reader/Writer/VectorIndex) 是 **创造 3 个新抽象来填一个无人使用的洞**, 加重 YAGNI 罪。

**证据**:
- 0 call site 作为 trait bound / dyn / 装箱 → trait 仅是 impl 块的"包容器"
- 10 方法 = 1 个 impl 块自带 10 个 inherent 方法, 不需要 trait 间接

**行动**: REMOVE trait, 保留 `SkillRegistry` 的 10 个 inherent 方法。

## 第 2 派: 架构派 (Architect)

**核心立场**: ISP 原则 + trait 表达力 = 拆分有价值, 但前提是 trait 有真实用户。

**对 SkillProvider**:
- ⚠️ 历史立场: 拆为 3 trait (Reader/Writer/VectorIndex)。**新立场: 同意 REMOVE**。
- 拆分价值的前提是 0 bound + 0 dyn → 拆出 3 个 0 bound + 0 dyn trait = **3 倍 YAGNI**。
- skill 系统活跃 = `SkillRegistry` 重要 ≠ `SkillProvider` trait 重要。
- 真要拆分, 应等到 skill 系统出现第二个 impl (e.g., `RemoteSkillProvider`) 或 dyn dispatch 需求 (e.g., `dyn SkillReader` 用于缓存层)。

**行动**: REMOVE trait (妥协, 但带 6-month revisit 条件: 若出现第二个 impl 或 dyn 需求, 重新评估 3-trait 拆分)。

## 第 3 派: 生产派 (Production)

**核心立场**: trait 价值在多后端/多场景时显现, 单 impl 时是负担。

**对 SkillProvider**:
- ✅ 同意删除。LLM skill 系统是差异化能力, 但 trait 抽象**本身**不提供差异化, 真实价值在 `SkillRegistry` 的实现 (dense/BM25/hybrid 匹配)。
- 0 dyn = 没有可热替换的扩展点需求。
- 0 trait bound = 没有任何代码利用 trait 抽象。
- 1 impl + 10 方法 = 公开 API 复杂, 但 trait 删除后 API surface 反而**减少** (调用方更明确知道"只有这一个实现")。

**行动**: REMOVE trait, 保留 impl。

## 第 4 派: 简化派 (Simplifier)

**核心立场**: YAGNI 极致形态 — 0 使用 = 0 价值 = 0 存在。

**对 SkillProvider**:
- ✅ 同意删除。和 SessionManager 100% 同构。
- 拆分方案 (Reader/Writer/VectorIndex) 是**架构完美主义**反例: 完美的抽象 + 0 真实用户 = 教科书级 YAGNI。
- 6-month revisit 也不是必要的, 因为 0 用户的数据不会因为时间推移而改变, 除非有具体新需求出现。

**行动**: REMOVE trait。

## 4 派共识

| 派别 | 立场 | 行动 |
|------|------|------|
| 怀疑派 | REMOVE | 删除 trait, 保留 inherent 方法 |
| 架构派 | REMOVE (妥协) | 同上, 6-month revisit 条件 |
| 生产派 | REMOVE | 删除 trait, 保留 impl |
| 简化派 | REMOVE | 删除 trait, 0 延迟 |

**共识**: **4-0 REMOVE**。

### 与 P0 SessionManager 决策一致性

| 项 | P0 SessionManager | P1 SkillProvider |
|----|-------------------|------------------|
| 决策 | REMOVE (4-0) | **REMOVE (4-0)** ✓ |
| 时间 | 2026-06-15 | 2026-06-15 |
| 模式 | 0 bound + 0 dyn + 1 impl + 0 Arc/Box | 同上 |
| 一致性 | ✅ | ✅ |

→ 本次决策与刚完成的 P0 一致, **不创造新的先例**, 延续同构问题的相同处理。

## 用户决策

待用户确认: 是否同意 4 派共识 (REMOVE trait, 转为 inherent 方法), 还是希望走原 P1 拆分方案?
