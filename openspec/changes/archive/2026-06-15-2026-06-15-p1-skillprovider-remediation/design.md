# Design: p1-skillprovider-remediation

## 1. 总览

1 个独立 sub-task, 1 个 commit, 删除 `pub trait SkillProvider` 全部引用。

```
Discovery (✅ done during setup)
   ↓
Delete trait + impl block + re-export
   ↓
Update 4 call sites (remove `use` imports)
   ↓
cargo check/test/clippy/fmt
   ↓
verify.md + archive
```

**为什么 1 commit**: 全部是删除, 无相互依赖, 无运行期影响, 1 个语义单元
("kill the dead trait")。1 commit 比 5 commit 更易 review。

## 2. 当前状态 (re-audit 2026-06-15)

### 2.1 Trait 定义

[crates/synthia-skill/src/traits.rs:8-33](file:///home/crochee/workspace/synthia/crates/synthia-skill/src/traits.rs#L8-L33):

```rust
#[async_trait]
pub trait SkillProvider: Send + Sync {
    fn list_skills(&self) -> Vec<SkillMetadata>;
    async fn get_skill(&self, name: &str) -> Result<Skill, Error>;
    async fn match_skills(&self, task_description: &str) -> Vec<SkillMatch>;
    fn register_from_path(&self, path: &Path) -> Result<(), Error>;
    fn unregister(&self, name: &str) -> bool;
    fn disable(&self, name: &str) -> bool;
    fn enable(&self, name: &str) -> bool;
    fn reload(&self, path: &Path) -> Result<(), Error>;
    fn match_skills_vector(
        &self, task_description: &str, top_k: usize,
    ) -> Vec<(String, f64)>;
    fn rebuild_vector_index(&self);
}
```

### 2.2 Trait 引用 (7 处)

| 文件 | 行 | 形式 | 用途 |
|------|---|------|------|
| `traits.rs:9` | 1 | `pub trait` 定义 | trait 本身 |
| `registry.rs:554` | 1 | `impl crate::traits::SkillProvider for SkillRegistry` | 唯一 impl |
| `lib.rs:24` | 1 | `pub use traits::SkillProvider;` | crate root re-export |
| `installer.rs:18` | 1 | `use traits::SkillProvider;` | import (未使用) |
| `watcher.rs:19` | 1 | `use traits::SkillProvider;` | import (未使用) |
| `implicit_tools.rs:247` | 1 | `use crate::{traits::SkillProvider, types::SkillPaths};` | test fixture import |
| `command/src/builtin/skill.rs:7` | 1 | `SkillProvider,` (multi-name import) | import (未使用) |

### 2.3 关键观察

- **0 trait bound**: 没有任何代码写 `T: SkillProvider` 或 `&dyn SkillProvider`
- **0 Arc/Box wrapping**: 没有任何代码把 `SkillProvider` 装箱
- **0 method calls via trait**: 全部通过 `SkillRegistry` 直接方法调用
- `use ... SkillProvider;` 出现在 4 个文件, 但**没有后续使用** — 是死 import
- `pub use traits::SkillProvider;` 在 `lib.rs` 是**死 re-export** (无下游消费)

### 2.4 与 P0 SessionManager 对比

| 维度 | P0 SessionManager | P1 SkillProvider |
|------|-------------------|------------------|
| impl | 1 | 1 |
| methods | 12 | 10 |
| 0 bound + 0 dyn + 0 Arc/Box | ✅ | ✅ |
| 唯一用户是 impl 自己 | ✅ | ✅ |
| 处理决策 | REMOVE (4-0) | **REMOVE (4-0)** |
| 处理时间 | 2026-06-15 | 2026-06-15 |

→ 100% 同构, 应同等处理。

## 3. 修改

### 3.1 删除 trait 定义

**`crates/synthia-skill/src/traits.rs`** (33 行 → 0 行, 文件可保留空壳或删除):

**方案 A (推荐)**: 删除整个文件 (`traits.rs` 仅含 trait 定义)
**方案 B**: 保留空 `traits.rs` 文件, 以便未来添加其他 trait (但本仓库无此 pattern)

选 A: 删除文件。

### 3.2 删除 impl block

**`crates/synthia-skill/src/registry.rs:553-?`**:

`impl crate::traits::SkillProvider for SkillRegistry` 块 (~120 行) 全部删除。
方法体**保留**为 inherent 方法 (`impl SkillRegistry { ... }` 块已存在, 把方法
从 trait impl 移到 inherent impl, 或保持原位 — 需看现有结构)。

**注**: `SkillRegistry` 的 inherent impl 块应在 `registry.rs` 中已存在。
trait impl 块删除后, 方法体留在原 impl 块中, 编译期需要：
- 去掉 `#[async_trait]` 宏 (因为 inherent impl 不需要)
- 保留方法签名 (与 trait impl 相同)
- 保留方法体 (不变)

**风险**: trait impl 中的方法有 `&self` 借用 + `Send + Sync` 要求, 转为 inherent 后
这些要求由 `SkillRegistry` 的结构体定义保证 (已经是 `Send + Sync`),
故**无功能差异**。

### 3.3 删除 crate root re-export

**`crates/synthia-skill/src/lib.rs:24`**:

```diff
 pub use registry::SkillRegistry;
 pub use tool_registry::register_skill_tool;
-pub use traits::SkillProvider;
 pub use types::*;
 pub use watcher::SkillWatcher;
```

### 3.4 删除 4 处 dead import

| 文件 | 行 | 修改 |
|------|---|------|
| `installer.rs:18` | 1 | 删除 `traits::SkillProvider,` 行 |
| `watcher.rs:19` | 1 | 删除 `traits::SkillProvider,` 行 |
| `implicit_tools.rs:247` | 1 | 删除 `, traits::SkillProvider` 子项 |
| `command/src/builtin/skill.rs:7` | 1 | 删除 `SkillProvider,` 行 |

### 3.5 公开 API 影响

**Breaking change**: 任何 `use synthia_skill::SkillProvider;` 都会编译失败。

**风险评估**:
- 在工作区内: grep 已确认 0 真实使用 (6 处 import + 1 处 re-export, 全部 dead)
- 在工作区外: `synthia_skill` 0.x 版本, 公开 API 可破坏 (semver 不保证)
- 替代: 调用方改用 `SkillRegistry` 直接方法调用 (方法签名不变)

## 4. 验证

| Gate | 命令 | 通过标准 |
|------|------|----------|
| Compile | `cargo check --workspace` | 0 errors |
| Test | `cargo test --workspace` | 2980/2980 OK (或基线) |
| Lint | `cargo clippy --all-targets --all-features --tests --all` | 0 warnings |
| Format | `cargo +nightly fmt --all --check` | clean |
| Spec | `openspec validate 2026-06-15-p1-skillprovider-remediation --strict` | valid |
| Grep | `grep -rn 'SkillProvider' crates/ --include='*.rs'` | 0 matches |

## 5. 风险

| 风险 | 缓解 |
|------|------|
| 工作区外有下游使用 `synthia_skill::SkillProvider` | 0.x 版本, semver 不保证, 在 changelog 中标注 BREAKING |
| `#[async_trait]` 宏行为差异 (trait impl vs inherent) | 检查: 现有 `impl SkillRegistry { async fn ... }` 是否也需要 `#[async_trait]`。若是, 保留宏。 |
| `Send + Sync` 要求丢失 | `SkillRegistry` 自身定义为 `Send + Sync` (在 `Arc<RwLock<...>>` 等容器中已强制), 不需要 trait 强制 |
| `async fn` 方法的 object safety (若未来需要 dyn) | trait 移除后, 不再支持 `dyn SkillProvider`。若未来需要 dyn, 用 `async-trait` crate + 新 trait |

## 6. 与 P0 SessionManager 的一致性

本次 change 与 `2026-06-15-p0-trait-review-remediation` Sub-task C
(SessionManager 删除) 是**100% 同构决策**:
- 同样的 trait 模式 (0 bound + 0 dyn + 1 impl + 0 Arc/Box)
- 同样的 4-0 REMOVE 共识
- 同一天的两次独立审计
- 同样的执行模式 (delete trait + impl + re-export + dead imports + 1 commit)

→ 决策一致性 ✅, 不创造新的先例。
