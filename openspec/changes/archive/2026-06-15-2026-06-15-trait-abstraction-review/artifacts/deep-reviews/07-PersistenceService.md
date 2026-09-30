# Deep Review: `PersistenceService`

**Location**: `crates/synthia-session/src/service.rs:20`
**Signals**: 1 impl / 7 methods / 0 generics / 0 call sites / 0 dyn

## 目的
Session 持久化后端抽象,7 个方法:`save_session`, `load_session`, `append_message`, `load_messages_recent<T>`, `load_messages_all<T>`, `save_checkpoint`, `load_checkpoint`。覆盖 session 元数据 + 消息日志 + checkpoint 三类持久化。

## 存在价值
- 1 impl: `Store` (in file_store.rs)
- 0 dyn 引用: 调用方直接用 `Store` 具体类型
- 7 个方法覆盖完整持久化 API,粒度合适

## 替代方案
- **A) 直接用 `Store`**: 失去后端可替换性 (in-memory, S3, Postgres 等)
- **B) 保留 trait + 简化**: 7 方法不可拆分 (元数据/消息/checkpoint 是 3 个独立子域)
- **C) 拆为多个 trait**: 可拆为 `SessionStore` (元数据) + `MessageLog` (消息) + `CheckpointStore`。**值得考虑**

## 推荐
**KEEP** (但考虑拆分为 3 个更小的 trait)

## 理由
虽然 1 impl + 0 dyn 是"预留"模式,但**7 个方法 + 异步 + 泛型方法**的 trait 在没有具体多后端需求时确实是负担。然而,session 持久化是 LLM 应用**最常见的多后端场景**(in-memory for dev, file for prod, S3/Postgres for cloud),trait 是合理的前瞻设计。**7 方法 + 2 泛型**的 trait 略大,值得拆为 3 个 focused trait (SessionStore / MessageLog / CheckpointStore) 以遵守 ISP。

## 4-party 检查

- **怀疑派**: 0 dyn + 7 方法 = 大 trait 仅为 1 实现。**REMOVE_CANDIDATE**。
- **架构派**: PBAC 类似,DIP 模式。KEEP。但粒度可拆。**KEEP + 拆分建议**。
- **生产派**: 多后端需求真实存在 (S3/Postgres),trait 价值在生产环境显现。**KEEP**。
- **简化派**: 7 方法过大,违反 ISP。可拆为 3 个小 trait。**KEEP + 拆分**。

**共识**: 3 派倾向 KEEP,怀疑派倾向 REMOVE。最终:**KEEP (with 拆分建议)**。

### 实现建议
- 拆分为 3 个 focused trait,各自独立 dyn dispatch:
  - `SessionStore` (save/load session)
  - `MessageLog` (append/load messages)
  - `CheckpointStore` (save/load checkpoint)
- `Store` 实现全部 3 个 trait
- 调用方按需用 dyn (e.g., agent loop 只需 `dyn MessageLog`)
