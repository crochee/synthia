# Developing — Synthia

> 本文档是开发者**深入指南**。第一次来的读者先读
> [`CONTRIBUTING.md`](../CONTRIBUTING.md)；选 seam / 改 trait 的
> 读者读 [`SEAMS.md`](../SEAMS.md)。本文是"如何在这套约束里写出可
> 维护的 Rust 代码"。

## 1. 仓库根目录常驻命令

| 做什么 | 命令 | 说明 |
|---|---|---|
| 全栈门禁 | `make ci` | fmt + clippy + rustdoc + tests + 9 项棘轮；CI 与本地一致 |
| 仅 lint | `make lint-rust` | `cargo clippy --all-targets --all-features --tests --all -- -D warnings` |
| 仅 fmt | `cargo +nightly fmt --all` | nightly（rustfmt.toml 要求） |
| 逐 crate 跑测试 | `make test-crates` | 禁止 `cargo test --workspace`（见 AGENTS.md §3.3） |
| 跑所有 example | `make examples` | 含两个独立 consumer crate 的 proof line |
| 跑 hot-path bench | `make bench-check` | 比例不是绝对值，跑在任意 runner 都成立 |
| 验证 MVP 依赖 | `make check-mvp-deps` | 默认 + 7 feature 子集 + 8 离线 tool crate 都无 HTTP/OTel/DB |
| 验证 lib 公开面无运行时类型 | `make check-public-api-runtime` | tokio / async-std / smol 别名到公开链的阻断 |
| 验证 pub glob re-export | `make check-pub-surface` | facede 之外不得 `pub use <module>::*;` |
| 验证时钟只走 sync trait | `make check-clock` | `Utc::now()` 在生产代码只允许 `#[cfg(test)]` 内 |
| 验证 harness 形状 | `make check-harness-shape` | synthia-harness 函数 ≤ 100 行、嵌套 ≤ 4 层 |
| 验证 test 内联块 ≤ 400 行 | `make check-test-layout` | 棘轮：内联 mod tests 不得超限 |

## 2. 改 trait / 公开 API 流程

```
crates/synthia-core/src/<name>.rs      ← trait 定义 / 类型定义
   │
   ▼
crates/<downstream>/src/             ← 实现 trait
   │
   ▼
crates/synthia/src/<module>.rs       ← 单一真源 re-export
   │
   ▼
crates/synthia-server/src/           ← 应用 crate 直接消费
synthia-web/src/                     ← 前端通过 fetch 间接消费
```

**关键约束**（见 `AGENTS.md` §3.7）：

1. **`synthia-core` 是终点的终点**：被 `synthia-*` 依赖，但**不能**反过来
   依赖任何 `synthia-*`；同样**不**直接依赖 `tokio` / `async-std` 等
   运行时。
2. **`synthia` facade 是唯一允许 `pub use <module>::*;`** —— 它本身
   不带逻辑，只搬运（`crates/synthia/src/*.rs`）；底层 crate 严禁
   glob re-export（`make check-pub-surface`）。
3. **改公开 API 必须**：
   - 跑 `xd://lsp` 看引用计数：definition + references + type definition；
   - 同步更新 `MINIMAL.md` / `SEAMS.md` / `CHANGELOG.md` `[Unreleased]` 段；
   - 跑 `make check-pub-surface` 与 `make check-public-api-runtime`。

## 3. 新增 crate 的"勾选清单"

按 `AGENTS.md` §3.7 与 §1：

- [ ] 加入 `Cargo.toml` 的 `[workspace.members]` 与 `[workspace.dependencies]`
- [ ] 自带 ≥ 1 个 `#[cfg(test)]` 单元测试（或独立的 `tests.rs`），
      `cargo test -p <crate> --lib` 绿
- [ ] 在 facade `crates/synthia/Cargo.toml` 暴露对应 feature flag
- [ ] 至少 1 个 `examples/<x>.rs`，最后一行打 `PROOF: OK`
      （`make examples` walker 自动收）
- [ ] `crates/<x>/README.md` 含：用途 + seam 映射 + 与现有 crate 的依赖方向
- [ ] 不引入与现有 crate 重复的 trait / type
- [ ] 通过 `make ci`

## 4. 新增工具插件

工具走 `synthia-tool` 范式：

```rust
// crates/synthia-tool-mything/src/lib.rs
use synthia_tool::{Tool, ToolDescriptor, ToolContext, ToolResult};
use synthia_macros::Tool;
use async_trait::async_trait;

#[derive(Tool, Default)]
#[tool(name = "my_thing", description = "Does the thing")]
pub struct MyThing;

#[async_trait]
impl Tool for MyThing {
    async fn execute(&self, ctx: ToolContext) -> ToolResult { /* ... */ }
}
```

注册到消费方：

```rust
let mut registry = ToolRegistry::new();
registry.register_entry(ToolEntry::new(Arc::new(MyThing::default())));
```

详细约束见 `crates/synthia-tool/README.md`。

## 5. 新增 Provider 适配器

走 `synthia-provider` trait：

```rust
use synthia_provider::{ModelProvider, ProviderRequest, ProviderResponse, StreamChunk};
use async_trait::async_trait;

pub struct MyProvider;

#[async_trait]
impl ModelProvider for MyProvider {
    async fn stream(&self, req: ProviderRequest) -> ProviderStream { /* ... */ }
}
```

- **不要**直接 wire up `reqwest::Client`；走 `synthia-provider::http::HttpClient`
  trait，方便 mock；
- 默认走 `OpenAICompatibleProvider`（已就位）；新增协议直接写新 adapter。

## 6. 写测试

- 测试**禁止** `cargo test --workspace`（AGENTS.md §3.3）；
- 内联 `#[cfg(test)] mod tests` 不得超过 400 行（`make check-test-layout`），
  超出放同级 `tests.rs`；
- 不写"绑手"测试：behaviour / 边界 / 不变量 / 状态转换 / 错误类。
  **禁**只测试 wiring / 转发 / 复制 / forward / 长度变长 / 重复同 path 行。
- 用 `SharedClock::fixed_at(t)` / `SharedIdGen::sequence()` 注入，
  不要 `chrono::Utc::now()` / `ulid::Generator::new().generate()`。

## 7. 改测试 / 写 fixture

- 协作者 fixture 用 `crates/synthia-test-support/`（已有）；
- 不要 `tokio::test!` —— 测试独立于运行时；用 `futures::executor::block_on`
  或 `#[tokio::test]` 看 crate 是否 runtime-free：
  - 默认无 tokio 的 crate：`futures::executor::block_on(async { ... })`；
  - 有 `tokio` 的 crate（`synthia-server`）：`#[tokio::test]`。

## 8. 与 CNCF 社区惯例保持一致

- **DCO 签核**（`git commit -s`）—— 见 [CONTRIBUTING.md](../CONTRIBUTING.md)；
- **Conventional Commits**（`feat:` / `fix:` / `docs:` / `refactor:` /
  `test:` / `chore:` / `perf:` / `ci:`）—— 见 [CONTRIBUTING.md](../CONTRIBUTING.md)；
- **PR 模板**：见 [.github/PULL_REQUEST_TEMPLATE.md](../.github/PULL_REQUEST_TEMPLATE.md)；
- **行为准则**：见 https://github.com/cncf/foundation/blob/main/code-of-conduct.md。

## 9. 调试与诊断

| 现象 | 工具 |
|---|---|
| `cargo` 在沙箱里 git fetch 失败 | 看 `.cargo/config.toml` 的 `git-fetch-with-cli = true` |
| OTLP 不出 span | 看 `SYNTHIA_OTLP_ENDPOINT` 与 `SYNTHIA_OTEL_SAMPLER` |
| 运行时依赖被偷拉到 lib | `make check-public-api-runtime` |
| 默认 feature 漏 HTTP 依赖 | `make check-mvp-deps` |
| harness 函数超 100 行 | `make check-harness-shape` |
| `chrono::Utc::now()` 进生产 | `make check-clock` |

## 10. 反馈与问题

- 设计讨论：[Discussions](https://github.com/crochee/synthia/discussions)；
- Bug 报告：[Issues](https://github.com/crochee/synthia/issues/new/choose)
  选 `bug_report`；
- 安全：[SECURITY.md](../SECURITY.md)；
- 其它：[SUPPORT.md](../.github/SUPPORT.md)。

---

> 本文件与 `AGENTS.md` §3.7 同步更新；改了工程约束（Rust toolchain /
> clippy 配置 / 形状预算 / 时钟策略）必须同 PR 更新 `AGENTS.md`。