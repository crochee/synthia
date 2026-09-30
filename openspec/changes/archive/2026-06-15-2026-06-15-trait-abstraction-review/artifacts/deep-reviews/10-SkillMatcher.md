# Deep Review: `SkillMatcher`

**Location**: `crates/synthia-skill/src/matcher.rs:9`
**Signals**: 1 impl / 1 methods / 0 generics / 0 call sites / 0 dyn

## 目的
技能匹配抽象,1 个方法 `match_skills(query, skills) -> Vec<SkillMatch>`。当前实现 `BM25Matcher` 用 BM25 算法匹配。

## 存在价值
- 1 impl: `BM25Matcher`
- 0 dyn 引用
- 未来可能: 向量匹配、LLM 匹配、混合匹配

## 替代方案
- **A) 直接用 `BM25Matcher`**: 失去算法可替换性
- **B) 保留 trait**: 1 方法已最小
- **C) 拆 trait**: 1 方法无法拆

## 推荐
**REMOVE_CANDIDATE** (保留 trait 但标记为低优先级)

## 理由
1 impl + 0 dyn 是 YAGNI 模式。BM25 是合理默认,但**未观察到切换需求**。然而,`SkillProvider` 已有 10 方法的复杂 trait,`SkillMatcher` 反而是合适的"算法可替换"边界(BM25 vs 向量)。trait 价值中等。

## 4-party 检查

- **怀疑派**: 0 dyn,YAGNI。**REMOVE_CANDIDATE**。
- **架构派**: 技能匹配是核心,可替换算法是合理设计。**KEEP**。
- **生产派**: 当前 1 算法够用,无切换需求。**REMOVE_CANDIDATE**。
- **简化派**: 1 方法 trait,直接用 `BM25Matcher::match_skills`。**REMOVE_CANDIDATE**。

**共识**: 3 派 REMOVE,1 派 KEEP。最终:**REMOVE_CANDIDATE**。

### 实现建议
```rust
// 替换为:
pub struct BM25Matcher { ... }
impl BM25Matcher {
    pub async fn match_skills(&self, query: &str, skills: &[Skill]) -> Vec<SkillMatch> { ... }
}
// 当新增向量/LLM 匹配器时,提取 trait(2 impl 自然出现)
```
