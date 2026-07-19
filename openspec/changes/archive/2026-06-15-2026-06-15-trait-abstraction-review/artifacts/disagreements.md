# 4-party Disagreements

> Recorded for transparency. Per design.md §6, disagreements are **not resolved** —
> they are kept for future OpenSpec changes to revisit.

## DoomLoopHandler (2-2 split)

**Classification**: REMOVE_CANDIDATE (decided by majority tiebreaker)

**Disagreement**:
- 怀疑派 (REMOVE): 0 dyn,纯预留,YAGNI。
- 架构派 (KEEP): doom loop 是策略点,安全核心,但承认"使用方缺失"。
- 生产派 (KEEP): 未来多 handler 概率高(LlmFeedback/UserConfirm/RateLimit)。
- 简化派 (REMOVE): 1 方法 + 0 dyn,直接具体类型即可。

**Resolution**: 倾向 REMOVE (推迟),但保留 KEEP 派意见。当第二 impl 出现时,自然引入 trait。

## PersistenceService (3-1 split)

**Classification**: KEEP (with 拆分建议)

**Disagreement**:
- 怀疑派 (REMOVE): 0 dyn + 7 方法 = 大 trait 仅为 1 实现。
- 架构派 (KEEP + 拆分): PBAC 类似,DIP 模式;但粒度可拆。
- 生产派 (KEEP): 多后端需求真实存在 (S3/Postgres),trait 价值在生产环境显现。
- 简化派 (KEEP + 拆分): 7 方法过大,违反 ISP。

**Resolution**: KEEP (with 拆分建议)。3 派支持 KEEP,但强烈建议拆分为 3 focused trait。

## ShellExecutor (3-1 split)

**Classification**: KEEP

**Disagreement**:
- 怀疑派 (REMOVE): 0 dyn + 1 impl,YAGNI 警告。
- 架构派 (KEEP): shell 是 sandbox 边界,安全关键。DIP 价值高。
- 生产派 (KEEP): Docker/sandbox 需求真实存在。
- 简化派 (KEEP): 当前调用方未使用 dyn,但 trait 价值在"未来需要时无需修改调用方"。

**Resolution**: KEEP。3 派支持。怀疑派警告记录。

## SkillMatcher (3-1 split)

**Classification**: REMOVE_CANDIDATE

**Disagreement**:
- 怀疑派 (REMOVE): 0 dyn,YAGNI。
- 架构派 (KEEP): 技能匹配是核心,可替换算法是合理设计。
- 生产派 (REMOVE): 当前 1 算法够用,无切换需求。
- 简化派 (REMOVE): 1 方法 trait,直接用 `BM25Matcher::match_skills`。

**Resolution**: REMOVE_CANDIDATE。3 派支持。架构派意见记录。

## SessionWriter (3-1 split)

**Classification**: REMOVE_CANDIDATE

**Disagreement**:
- 怀疑派 (REMOVE): 1 真实 impl (NoOp) + 1 dyn,本质是"开关"。
- 架构派 (KEEP, 低优先级): noop + trait 是合理 "feature flag" 模式。但当前所有调用方都是 noop。
- 生产派 (REMOVE): 1 dyn 但全是 noop,生产价值 0。
- 简化派 (REMOVE): 简化为 `Option<NoOpSessionWriter>` 即可。

**Resolution**: REMOVE_CANDIDATE。3 派支持。架构派意见记录。

---

## Summary

5 个争议项,均已通过多数票决议。所有 KEEP 派意见保留为后续决策参考。
