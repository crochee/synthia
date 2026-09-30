# Deep Review: {TraitName}

**Location**: `crates/.../foo.rs:N`
**Signals**: {impl} impl / {methods} methods / {gen} generics / {calls} call sites / {dyn} dyn

## 目的
{1-2 句:从 doc comment + 实际 usage 推断这 trait 解决什么问题}

## 存在价值
{解释 why this trait vs 直接用具体类型。列出至少 1 个具体使用场景 (文件:行号)}

## 替代方案
- **A) 直接用具体类型** (无 trait)。代码量减少, 但失去多态能力
- **B) 保留 trait + 简化方法集**。如果 method_count > 5, 看是否职责过宽
- **C) 拆为多个小 trait** (接口隔离)。如果 generic_params >= 2, 看是否能拆

## 推荐
**{KEEP | REVIEW | REMOVE_CANDIDATE}**

## 理由
{2-3 句,基于具体证据 (impl 数 / 调用点 / 历史 commit / 未来 plan)。
例如: "impl=1 + call_sites=0, 但有 dyn 引用 → KEEP 但需记录"}

## 4-party 检查

- **怀疑派** (默认移除): {立场 + 论证}
- **架构派** (依赖倒置): {立场 + 论证}
- **生产派** (影响面): {立场 + 论证}
- **简化派** (更简单的抽象): {立场 + 论证}

**共识**: {N 派同意 / 分歧记录}
