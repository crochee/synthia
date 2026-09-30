# Design: trait-abstraction-review

> Version: 1.0
> Date: 2026-06-15
> Method: Hybrid (A: Heuristic 全扫 + C: 重点深掘)

## 1. 整体架构

```
openspec/changes/2026-06-15-trait-abstraction-review/
├── README.md
├── proposal.md            # Why/What/Impact
├── design.md              # 本文件
├── tasks.md               # 7 阶段任务
├── verify.md              # 验收清单
├── brainstorm.md          # 4 派对抗原始记录
├── scripts/
│   └── extract_trait_signals.sh   # Phase 1 采集脚本 (0 依赖)
├── specs/
│   └── trait-abstraction-review/
│       └── spec.md        # 交付物要求 (SHALL/MUST)
└── artifacts/             # Phase 2-7 产出
    ├── trait-inventory.md
    ├── deep-reviews/
    │   ├── 01-{trait}.md
    │   └── ...
    ├── recommendations.md
    └── disagreements.md
```

## 2. 8 信号定义 (Phase A)

每个 `pub trait` 自动采集:

| 信号 | 提取方式 | 危险阈值 |
|------|----------|----------|
| `impl_count` | `grep -c "impl<...> {Name} for"` | 1 → YAGNI 嫌疑 |
| `method_count` | `awk` 计数 trait 块内 `fn` (减 pub trait 头) | >5 → 职责过宽 |
| `generic_params` | trait `<T, U, ...>` 解析 | >1 → 抽象过载 |
| `lifetime_params` | `<'a, 'b, ...>` | >1 → 借用复杂 |
| `associated_types` | `type Foo =` 计数 | >2 → 设计复杂 |
| `call_sites` | `grep -r "as {Name}"` 加 `dyn {Name}` 计数 | 0 → dead |
| `dyn_usage` | `grep "dyn {Name}"` | 0 → 不需要 dyn dispatch |
| `file_size_lines` | trait 块行数 (从 `{` 到匹配 `}`) | >80 → scope creep |

**采集脚本规范**:
- 0 依赖: 仅 `rg` + `awk` + `bash`
- 输出 UTF-8 markdown 表 (pipe-delimited safe)
- self-test: 准备 1-2 个 fixture trait 块, 验证输出 8 列齐全

## 3. 决策矩阵 (Phase 3 分流)

| impl | calls | generic | 类别 | 深度 review? |
|------|-------|---------|------|--------------|
| 1 | <3 | 0 | REMOVE_CANDIDATE | **是** |
| 1 | ≥3 | any | REVIEW (单实占主流) | **是** |
| 1 | any | ≥2 | REVIEW (泛型重) | **是** |
| 2+ | any | <2 | KEEP | 跳过 |
| 2+ | any | ≥2 | REVIEW (泛型重) | **是** |
| 2+ | high | 0 | KEEP | 跳过 |
| 0 calls | any | any | KEEP-dead? | 检查 dyn_usage |

**预期分布** (粗估):
- KEEP: 35-40 个
- REVIEW: 10-15 个
- REMOVE_CANDIDATE: 5-10 个
- Deep review 实际数: 10-15 个 (受时间盒约束,上限 15)

## 4. Deep review 模板 (Phase C)

```markdown
## {Trait 名}

**位置**: `crates/.../foo.rs:N`
**信号**: 1 impl / 4 methods / 0 generics / 12 call sites / 0 dyn

### 目的
{从 doc/usage 推断,1-2 句}

### 存在价值
{解释 why this trait vs concrete type, 列出至少 1 个具体使用场景}

### 替代方案
- A) 直接用具体类型 (无 trait)
- B) 保留 trait + 简化方法集
- C) 拆为多个小 trait (按接口隔离)

### 推荐
**{KEEP | REVIEW | REMOVE_CANDIDATE}**

### 理由
{2-3 句,基于具体证据 (impl 数/调用点/历史 commit/未来 plan)}

### 4-party 检查
- 怀疑派: {立场 + 论证}
- 架构派: {立场 + 论证}
- 生产派: {立场 + 论证}
- 简化派: {立场 + 论证}

**最终共识**: {≥ 3 派同意 / 分歧见 disagreements.md}
```

## 5. 7 阶段执行流

| 阶段 | 内容 | 验证 | 时间 |
|------|------|------|------|
| 1 | 写 `extract_trait_signals.sh` | self-test (fixture) | 20m |
| 2 | 跑脚本产出 `trait-inventory.md` | 56 行 + 8 列齐全 | 5m |
| 3 | 按决策矩阵分流,选 deep review 名单 | 数量 10-15 | 5m |
| 4 | 每个候选走 deep review 模板 | 4-party 检查 | 75m |
| 5 | 汇总 `recommendations.md` | 三类加和 = 56 | 15m |
| 6 | 4-party 对抗审查整个报告 | 共识 ≥ 3 派 / 写 disagreements.md | 15m |
| 7 | 写 `verify.md` + `openspec validate` | 通过 | 20m |

## 6. 4-party 对抗审查 (沿用项目惯例)

每个深 review 必须过 4 派审查,≥ 3 派同意分类:

- **怀疑派**: 质疑 trait 是否真有必要 (默认移除)
- **架构派**: 评估是否符合 SOLID / 是否符合依赖倒置
- **生产派**: 评估移除/重构的影响面 (下游使用方)
- **简化派**: 评估能否用更简单的抽象 (具体类型 / 闭包 / newtype)

分歧写入 `artifacts/disagreements.md`, 不消除 (留痕供未来决策参考)。

## 7. Out-of-scope (本 change 明确不做)

- ❌ 不实施 trait 重构/移除
- ❌ 不创建新 trait
- ❌ 不修改任何 `src/` 业务代码
- ❌ 不改公开 API
- ❌ 不动 `archive/` 已归档 change

## 8. 风险与缓解

| 风险 | 缓解 |
|------|------|
| 评审主观偏差 (impl=1 一定坏?) | 4-party 对抗 + 深 review 显式记录 "为何 1 impl 合理" |
| 信号失真 (call_sites=0 不一定 dead) | 检查 dyn_usage + cfg(test) 后再判断 |
| 时间溢出 (15 × 5min 实际更长) | 硬时间盒 + 数量上限 15 (决策矩阵过滤) |
| 漏数 (脚本 bug 漏掉 1 个 trait) | Phase 5 sum-to-56 验证 + 脚本 self-test |

## 9. 硬约束

- 零新依赖 (沿用 `rg` + `bash`)
- 输出 100% 在 `openspec/changes/2026-06-15-trait-abstraction-review/` 内
- 不动 `src/` 任何文件
- 7 阶段全部有 self-test / 验证, 不跳过
- 沿用现有 `agents-md-hierarchical-discovery` 的 OpenSpec 结构

## 10. 依赖项

- `openspec` CLI 已配置 (1.3.1, 已知 bug: 数字开头 change name 在
  `status`/`instructions apply` 中被拒, 但 `list`/`validate`/`archive`
  接受; 本 change 名以 `2026-06-` 开头, 符合; 若 status 命令被拒,
  改用 `openspec list --json` 自行 grep)
- `rg` (项目已用)

## 11. Open questions

- 是否在 `recommendations.md` 末尾留 "Future refactor candidates"
  索引段, 作为后续 OpenSpec change 的种子?

  **建议**: 留, 1 段 30 行, 只列 trait 名 + 推荐类别 + 优先级 (P0/P1/P2)
