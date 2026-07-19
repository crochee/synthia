# Deep Review: `SessionManager`

**Location**: `crates/synthia-session/src/session.rs:110`
**Signals**: 1 impl / 12 methods / 0 generics / 0 call sites / 1 dyn

## 目的
Session 生命周期管理抽象,12 个方法: `get_session`, `create_session`, `update_session`, `delete_session`, `add_message`, `get_conversation`, `get_recent_conversations`, `get_conversation_messages`, `replace_conversation`, `fix_conversation`, `get_message_count` (+ 更多未显示)。

## 存在价值
- 1 impl: `Store` (实际是 `Store` 实现整个 `SessionManager` trait)
- 1 dyn 引用 (实际是从 `use crate::session::{...SessionManager as SessionManagerTrait}` 解析的,实际 dyn 使用见下方核实)
- 12 方法 - **最大的 trait** 之一

## 替代方案
- **A) 直接用 `Store`**: 失去后端可替换性
- **B) 保留 trait + 简化**: 12 方法过大,无法简化
- **C) 拆为多个 trait (强烈推荐)**:
  - `SessionStore` (CRUD session)
  - `MessageStore` (add/get/replace messages)
  - `ConversationQuery` (get_conversation, get_recent, fix)

## 推荐
**REVIEW** (高优先级拆分)

## 理由
**12 方法 + 1 dyn** 是教科书 ISP 违反。Session 抽象已与 `PersistenceService` (7 方法) 重叠 — 同样是 session 持久化,但分散在 2 个 trait。**职责划分混乱**:
- `SessionManager`: 业务逻辑 (create/update/delete/fix)
- `PersistenceService`: 存储后端 (save/load/checkpoint)

但 `Store` 同时实现两者,代码逻辑必然重叠。强烈建议**合并/拆分**两 trait。

## 4-party 检查

- **怀疑派**: 12 方法 + 1 dyn,严重违反 ISP。**REMOVE_CANDIDATE**。
- **架构派**: 与 PersistenceService 重叠,需重构。**REVIEW (拆分/合并)**。
- **生产派**: 真实使用频繁(1 dyn),trait 价值存在但粒度错误。**REVIEW (拆分)**。
- **简化派**: 12 方法是抽象过载的极致。**REVIEW (拆分)**。

**共识**: 4 派一致 (4-0) — **REVIEW (拆分)**。

### 实现建议 (P0 重构)
1. **合并/拆分** `SessionManager` 和 `PersistenceService`:
   - 保留 `PersistenceService` (纯存储后端,7 方法)
   - 删除 `SessionManager`,将业务方法移至具体 `Store` 或独立 `SessionService` (非 trait)
2. 或: 拆 `SessionManager` 为 3 个 focused trait (SessionStore/MessageStore/ConversationQuery)
3. **`Store` 应只实现 `PersistenceService`**,业务方法 (create_session, fix_conversation 等) 在更高层

### 风险
- 改动面大(全 session 系统的核心)
- 与 PersistenceService 重构耦合,建议同步进行
- breaking change,需 1.0 升级窗口
