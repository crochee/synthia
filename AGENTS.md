# AGENTS.md

本文件汇总项目环境与编码规范，供所有 agent 统一遵循。

***

# 1. 环境

- 真实 LLM API 配置位于仓库根目录的 `.env` 文件中。
- 可观测性栈（logging / trace / metrics）的依赖版本与 feature 集合统一由仓库根 [Cargo.toml](file:///home/crochee/workspace/synthia/Cargo.toml) `[workspace.dependencies]` 收口，始终拉起，调用方按需调用。详见 [crates/synthia-telemetry/README.md](crates/synthia-telemetry/README.md)。
- [synthia-server](file:///home/crochee/workspace/synthia/crates/synthia-server/) 自身保留两个 feature，仅用于 gate 自身的 wiring：
  - `synthia-server/otel` — 控制 OTLP trace-context 提示等 wiring。
  - `synthia-server/metrics` — gate `prometheus` + 挂载 `GET /metrics` 端点与 `track_metrics` 中间件。
- OpenTelemetry tracing 通过环境变量配置：
  - `SYNTHIA_OTLP_ENDPOINT` — OTLP collector 地址，scheme 自动选择 gRPC/HTTP（`grpc://` / `https://` / 无 scheme → gRPC；`http://` → HTTP，4317 端口例外走 gRPC）。未设置时退化为 console tracing。
  - `SYNTHIA_OTEL_SAMPLER` — 采样器覆盖（`always_on` / `always_off` / `trace_id_ratio:0.1`），默认 `ParentBased(AlwaysOn)`。设置后包裹 `ParentBased` 以兼容父 trace 采样决策。

***

# 2. 代码同步规范

- 不主动 push 代码到远程仓库。
- 搜索路径优先级：本地工作空间 > HOME 目录 > 其他目录。

***

# 3. Rust 编码规范

## 3.1 强制要求

- 所有 Rust app 应用只能使用 `anyhow` 进行错误处理，不使用 `thiserror`。
- 所有 Rust lib 库只能使用 `thiserror` 进行错误处理，不使用 `anyhow`。
- 所有依赖（直接 + 传递）一律使用 `major.minor` 格式（例：`1.1.0` → `1.1`），禁止完整三段或单段版本号。
- 引入新依赖时优先复用 [Cargo.toml](file:///home/crochee/workspace/synthia/Cargo.toml) 中 `[workspace.dependencies]` 已声明的依赖；crate 内通过 `dep = { workspace = true }` 引用，禁止各自写版本号。
- 必须满足 workspace 声明的 MSRV `rust-version = "1.95"`；引入依赖前确认其 MSRV 不高于本项目。

## 3.2 代码质量与格式化

- 新产生的 Rust 代码若未使用请直接删除；不得使用 `dead_code` / `unused` 等属性抑制警告。
- 控制圈复杂度，优先可读性与可维护性；遵循 Rust 官方编码风格指南（Rust Style Guide）。
- 每次完成编写后必须执行：

  ```bash
  cargo +nightly fmt --all
  ```
- 格式化后必须执行并修复所有警告与错误：

  ```bash
  cargo clippy --all-targets --all-features --tests --all
  ```

## 3.3 测试与编译规范

- 测试**禁止**一次性执行 `cargo test --workspace`；必须按模块分批执行：`cargo test -p <模块名> <...>`。
- 发现磁盘占用率过高时清理无用文件和构建产物（例如 `cargo clean`），避免 WSL 崩溃。

## 3.4 项目基线配置（单点真相）

下列配置已统一在仓库根目录文件中，是所有 crate 必须遵守的基线；agent 修改前应阅读相关文件而非各自复制：

- [rust-toolchain.toml](file:///home/crochee/workspace/synthia/rust-toolchain.toml) — 固定使用 `stable` 通道；`cargo +nightly fmt --all` 是格式化的特例（rustfmt 配置要求 nightly），其余命令一律使用 stable。
- [rustfmt.toml](file:///home/crochee/workspace/synthia/rustfmt.toml) — edition `2024`、`max_width = 80`、`imports_granularity = "Crate"`、`group_imports = "StdExternalCrate"`、`use_small_heuristics = "Default"` 等。
- [clippy.toml](file:///home/crochee/workspace/synthia/clippy.toml) — `cognitive-complexity-threshold = 20`、`warn-on-all-wildcard-imports = true`；在测试代码中允许 `expect` / `unwrap` / `dbg`；禁用 `map_or / map_or_else / for_each / try_for_each` 及 `std::mem::forget` / `std::ptr::read_unaligned`。
- [Cargo.toml](file:///home/crochee/workspace/synthia/Cargo.toml) — workspace `resolver = "2"`、`edition = "2024"`、`rust-version = "1.95"`；release profile 已固定 `opt-level = 3` / `lto = "thin"` / `codegen-units = 1` / `strip = "symbols"`，**不得随意调整**。
- [deny.toml](file:///home/crochee/workspace/synthia/deny.toml) — `cargo-deny` 的许可证 / advisory / ban 审计入口（占位配置）；新增依赖须保证通过该审计。

## 3.5 圈复杂度与代码异味

- 函数 / 方法的认知复杂度阈值遵循 [clippy.toml](file:///home/crochee/workspace/synthia/clippy.toml) 的 `cognitive-complexity-threshold = 20`；超出时优先拆分而非放宽阈值。
- 禁止使用通配符导入（`use foo::*;`），由 `warn-on-all-wildcard-imports = true` 强制。
- 优先使用 `for` 循环处理副作用场景，而非 `for_each` / `try_for_each`。
- 优先 `map(..).unwrap_or(..)` / `map(..).unwrap_or_else(..)`，而非 `map_or` / `map_or_else`。
- 禁止 `std::mem::forget`（内存泄漏风险）与 `std::ptr::read_unaligned`（unsafe 指针操作）。
- 编写 doc comment 时遵守 `doc-valid-idents` 白名单（如 `MiB`、`IPv4`、`OAuth`、`PostgreSQL` 等），避免触发 `clippy::doc_markdown` 警告。

## 3.6 自动化入口（Makefile）

优先通过仓库根目录的 [Makefile](file:///home/crochee/workspace/synthia/Makefile) 触发质量门禁，避免在脚本里拼装裸命令：

| 目标               | 作用                                                                  |
| ---------------- | ------------------------------------------------------------------- |
| `make fmt-rust`  | 格式化 Rust 代码（`cargo +nightly fmt --all`）                             |
| `make lint-rust` | Clippy（`--all-targets --all-features --tests --all -- -D warnings`） |
| `make test-unit` | 运行单元测试（`cargo test --workspace --lib`，CI 友好）                        |
| `make test-wire` | 契约回归测试（`cargo test -p synthia-core --features http,axum`）           |

任何对 `clippy.toml` / `rustfmt.toml` / `deny.toml` / `Cargo.toml` 的修改必须在同一变更中更新本文件对应引用，避免规则与配置漂移。

***

# 4. Web 前端编码规范

## 4.1 基本要求

- 0 lint（无 lint 错误/警告）。
- 需要格式化。
- 能运行起来。
- 能通过测试。
- 代码符合 Web TS / HTML / CSS 编码规范。
- 整体通过 UI 测试。

## 4.2 前端页面效果优化

- 优化前端页面效果时尽量自行操作浏览器进行验证。
- 优化时要考虑用户的操作习惯。
- 可使用 Playwright 测试前端页面效果。