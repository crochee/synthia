# Deep Review: `SkillProvider`

**Location**: `crates/synthia-skill/src/traits.rs:9`
**Signals**: 1 impl / 10 methods / 0 generics / 0 call sites / 0 dyn

## 目的
技能提供器抽象,10 个方法:`list_skills`, `get_skill`, `match_skills`, `register_from_path`, `unregister`, `disable`, `enable`, `reload`, `match_skills_vector`, `rebuild_vector_index`。

## 存在价值
- 1 impl: `SkillRegistry` (in registry.rs:554)
- 0 dyn 引用
- **10 个方法** — 涵盖 CRUD + 匹配 + 向量检索全套

## 替代方案
- **A) 直接用 `SkillRegistry`**: 失去可替换性
- **B) 保留 trait + 简化方法集**: 10 方法可能可削减 (`match_skills` vs `match_skills_vector` 是否冗余?)
- **C) 拆为多个 trait (ISP)**: 强烈推荐 — 至少 3 个 focused trait:
  - `SkillReader` (list/get/match_skills)
  - `SkillWriter` (register/unregister/disable/enable/reload)
  - `SkillVectorIndex` (match_skills_vector/rebuild_vector_index)

## 推荐
**REVIEW** (高优先级拆分)

## 理由
**10 方法 + 0 dyn** 是最严重的"胖 trait 预留"反例。Skill 系统有 3 个独立关注点(读取/写入/向量索引),trait 违反 ISP。LLM skill 系统是 LLM agent 的差异化能力,**trait 价值真实存在**但粒度错误。强烈建议拆为 3 个 focused trait,各 trait 单独 dyn dispatch,使用方按需依赖。

## 4-party 检查

- **怀疑派**: 10 方法 + 0 dyn = 教科书级反例。**REMOVE_CANDIDATE**。
- **架构派**: 违反 ISP,需拆为 3 trait。**REVIEW (拆分)**。
- **生产派**: skill 系统确实有 3 个独立能力,拆分有价值。**REVIEW (拆分)**。
- **简化派**: 1 个 trait 10 方法 = 抽象过载,违反 SRP/ISP。**REVIEW (拆分)**。

**共识**: 4 派一致 (4-0) — **REVIEW (拆分)**。

### 实现建议 (P1 重构)
```rust
pub trait SkillReader: Send + Sync {
    fn list_skills(&self) -> Vec<SkillMetadata>;
    async fn get_skill(&self, name: &str) -> Result<Skill, Error>;
    async fn match_skills(&self, task_description: &str) -> Vec<SkillMatch>;
}

pub trait SkillWriter: Send + Sync {
    fn register_from_path(&self, path: &Path) -> Result<(), Error>;
    fn unregister(&self, name: &str) -> bool;
    fn disable(&self, name: &str) -> bool;
    fn enable(&self, name: &str) -> bool;
    fn reload(&self, path: &Path) -> Result<(), Error>;
}

pub trait SkillVectorIndex: Send + Sync {
    fn match_skills_vector(&self, task_description: &str, top_k: usize) -> Vec<(String, f64)>;
    fn rebuild_vector_index(&self);
}

// SkillRegistry impl all 3
// 调用方按需用 dyn (e.g., 任务执行时只需 dyn SkillReader)
```

### 风险
- 拆分需修改所有调用点 (约 5-10 处)
- 公开 API 变化 (breaking change)
- 建议在 0.x → 1.0 升级窗口完成
