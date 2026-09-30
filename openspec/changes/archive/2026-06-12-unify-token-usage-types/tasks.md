## 1. 升级 synthia-provider::types::TokenUsage 为 canonical type

- [ ] 1.1 在 `crates/synthia-provider/src/types.rs:400` 的 `TokenUsage` derive 列表中加入 `Serialize, Deserialize`
- [ ] 1.2 给 `cached_prompt_tokens: Option<usize>` 字段添加 `#[serde(default)]` 属性
- [ ] 1.3 在 `TokenUsage` 文档注释中标注 "canonical type, used by all crates"
- [ ] 1.4 运行 `cargo check -p synthia-provider` 确认编译通过

## 2. 替换 synthia-session::types::TokenUsage 为 1-line shim

- [ ] 2.1 在 `crates/synthia-session/src/types.rs` 删除 `pub struct TokenUsage { ... }` 定义（约 6 行）
- [ ] 2.2 在 `crates/synthia-session/src/types.rs` 添加 `pub use synthia_provider::types::TokenUsage;` 单行 shim
- [ ] 2.3 运行 `cargo check -p synthia-session` 确认编译通过
- [ ] 2.4 运行 `cargo test -p synthia-session` 确认 12+ 现有测试通过（特别是 `session_manager_integration.rs:273, 282, 291` 的 `synthia_session::TokenUsage` 用法）

## 3. 替换 synthia-agent::events::TokenUsage 为 1-line shim

- [ ] 3.1 在 `crates/synthia-agent/src/events.rs` 删除 `pub struct TokenUsage { ... }` 定义（约 5 行）
- [ ] 3.2 在 `crates/synthia-agent/src/events.rs` 添加 `pub use synthia_provider::types::TokenUsage;` 单行 shim
- [ ] 3.3 运行 `cargo check -p synthia-agent` 确认编译通过
- [ ] 3.4 运行 `cargo test -p synthia-agent` 确认现有测试通过
- [ ] 3.5 验证 `crates/synthia-agent/src/stream_builder/builder.rs:413, 479` 的 `crate::events::TokenUsage { ... }` 构造调用继续工作

## 4. 删除 synthia-context::checkpoint::TokenUsageSnapshot

- [ ] 4.1 在 `crates/synthia-context/src/checkpoint.rs` 删除 `pub struct TokenUsageSnapshot { ... }` 定义（约 6 行）
- [ ] 4.2 替换 `Checkpoint.token_usage: TokenUsageSnapshot` 为 `Checkpoint.token_usage: synthia_provider::types::TokenUsage`
- [ ] 4.3 替换所有 `TokenUsageSnapshot` 引用为 `synthia_provider::types::TokenUsage`
- [ ] 4.4 验证 `Checkpoint::default()` 测试继续通过
- [ ] 4.5 运行 `cargo check -p synthia-context` 确认编译通过
- [ ] 4.6 运行 `cargo test -p synthia-context` 确认现有测试通过
- [ ] 4.7 运行 `grep -rn "TokenUsageSnapshot" crates/` 确认 0 处使用

## 5. 跨 crate 编译与测试

- [ ] 5.1 运行 `cargo check --workspace` 确认 0 错误
- [ ] 5.2 运行 `cargo test --workspace` 确认所有测试通过（特别是涉及 32 处引用的测试文件）
- [ ] 5.3 运行 `cargo build --release --workspace` 确认 release 编译通过
- [ ] 5.4 验证 `synthia_server::tests::e2e_server_sse_test::137`、`synthia_agent::tests::e2e_event_sequence_test::437` 等外部 import 路径继续工作

## 6. 代码质量与 OpenSpec 收尾

- [ ] 6.1 运行 `cargo +nightly fmt --all` 确保格式统一
- [ ] 6.2 运行 `cargo clippy --all-targets --all-features --tests --all` 修复所有警告
- [ ] 6.3 验证 grep 审计：`grep -rn "pub struct TokenUsage" crates/` 仅返回 1 处（provider），`grep -rn "TokenUsageSnapshot" crates/` 返回 0 处
- [ ] 6.4 在 `crates/synthia-agent/src/checkpoint.rs` 添加测试验证 `token_usage: TokenUsage` 字段类型为 `synthia_provider::types::TokenUsage`
- [ ] 6.5 在 `crates/synthia-context/src/checkpoint.rs` 添加测试验证 `Checkpoint` 序列化/反序列化 roundtrip 包含 `cached_prompt_tokens` 字段
- [ ] 6.6 运行 `openspec validate unify-token-usage-types --strict` 验证 OpenSpec 格式
- [ ] 6.7 提交 commit 并 push，commit message 格式遵循项目历史约定
- [ ] 6.8 运行 `openspec archive unify-token-usage-types` 归档（仅在所有任务完成且测试通过后）
</content>
</invoke>