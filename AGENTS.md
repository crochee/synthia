# AGENTS.md

本文件汇总项目环境与编码规范，供所有 agent 统一遵循。

> **关于仓库根的隐藏目录**：`./.agents/` `./.omo/` `./.superpowers/` `./.synthia/` `./.trae/` `./.worktrees/` `./.attachments/` `./.cargo/` 是**AI agent / 本地工具链的状态 / 草稿 / 会话**，不是本项目资产；均在 [`.gitignore`](.gitignore) 中显式排除。**不要**把它们的文件 / 路径写入 commit message、PR 描述或本文件；它们的存在不影响仓库对位 4w+ star 项目的"完整度"。同理 `./.env` 含真实 LLM API 配置，仅本地加载，**不入库**。

***

# 1. 环境

- 真实 LLM API 配置位于仓库根目录的 `.env` 文件中。
- 可观测性栈（logging / trace / metrics）的依赖版本统一由仓库根 [Cargo.toml](file:///home/crochee/workspace/synthia/Cargo.toml) `[workspace.dependencies]` 收口；是否启动由 **crate 级 feature** 决定，不再"始终拉起"：
  - `synthia-telemetry` 常驻编译 console / file logging（`tracing` + `tracing-subscriber`）；
  - `synthia-telemetry/metrics` — gate `prometheus` 向量与 `gather_text()`；
  - `synthia-telemetry/otlp` — gate OTel exporter、`tracer` / `propagation` 模块及全部 `opentelemetry*` + 传输依赖。
  - 仓库内成员显式声明所需 feature：`synthia-tool` 仅 `metrics`（保持 agent 依赖树无 exporter），`synthia-server` 显式 `["metrics", "otlp"]`；facade 的 `telemetry` feature 同时打开两者。详见 [crates/synthia-telemetry/README.md](crates/synthia-telemetry/README.md)。
- [synthia-server](file:///home/crochee/workspace/synthia/crates/synthia-server/) 对自身而言可观测性始终开启（OTLP tracing、Prometheus RED 指标、`GET /metrics` 端点与 `track_metrics` 中间件），仅保留一个 `test-utils` feature（转发 facade 的 `test-support`）用于测试 wiring。
- **synthia-server 只依赖 `synthia` facade**（R121 单一收口规则）：`crates/synthia-server/Cargo.toml` 的 synthia 依赖只有 `synthia = { workspace = true, features = [...] }` 一行，全部 SDK 类型（provider / harness / tool / session / skill / attachment / mcp / telemetry / core / 各 tool 插件）一律经 `synthia::module::…` 路径引用，不再直接依赖任何 `synthia-*` 库 crate。因此 facade **没有** `server` feature——facade 反向 re-export `synthia-server` 会构成依赖环；需要部署二进制的消费方在 `synthia` 之外自行添加 `synthia-server`。
- 网络传输（HTTP client）一律通过 crate 级 feature 可选，禁止无条件依赖 `reqwest`：
  - `synthia-provider/anthropic`、`synthia-provider/openai` — 两个内置 adapter（各自拉起 `reqwest` 与 `synthia-core/reqwest`），默认开启；只写自己的 `ModelProvider` 时用 `default-features = false`。
  - `synthia-tool-web` — `web_fetch` 工具，唯一允许拉起 `reqwest` 的 tool crate；其余 tool crate（`synthia-tool-read` / `-write` / `-shell` / `-todo` / `-task` / `-scheduler` / `-search`）保持离线。
  - facade 对应 feature：`provider-anthropic` / `provider-openai` / `tool-read` / `tool-write` / `tool-shell` / `tool-todo` / `tool-web` / `tool-task`（均在默认集合内）；`provider` 仅指 trait 与 wire types，`tool` 仅指范式（registry + `Tool` trait + `workspace` 路径约束），**不含面向 agent 的内建工具集**——`read` / `write` / `shell` / `todo` / `web` / `task` 各自独立成 crate；范式 crate 内只保留两个合成契约工具（`RetrieveFullOutputTool`、`StructuredOutputTool`，后者由 `AgentBuilder::output_schema` 自动注入）与 registry 的 `ToolEntry::dynamic` 直通实现。`tool-scheduler`（`synthia-tool-scheduler`）、`tool-search`（`synthia-tool-search`）与 `mcp` 一样是**可选** feature：三者的宿主都必须先构造运行时对象（`ScheduleStore` / 待检索的 `synthia-search::Registry` / `McpSupervisor`），因此不进入默认集合。`cron` 同样是 opt-in feature（`cron = 0.17`，`synthia-tool-scheduler/cron` 转发 `synthia-scheduler/cron`）：默认构建零 cron 解析器——`JobKind::Cron` 只退回一分钟占位、`schedule` 工具逐字拒绝 `kind: "cron"`。facade 的 `cron` feature 把这条路径**两半一起**打开（`scheduler` + `synthia-scheduler/cron` + `tool-scheduler` + `synthia-tool-scheduler/cron`）：只开解析器而工具仍拒绝 `kind: "cron"` 是半截切换，本轮"模型能创建 cron 任务"这个用户可见收益就落不了地；它不在默认集合内。立场文本在 [crates/synthia-scheduler/src/lib.rs](crates/synthia-scheduler/src/lib.rs) 与 [crates/synthia-tool-scheduler/src/lib.rs](crates/synthia-tool-scheduler/src/lib.rs)。
  - 仓库内成员必须显式声明所需 feature（`synthia-server` 经 facade 一行声明全集，见上；其余成员按各自 Cargo.toml）。`make check-mvp-deps` 保证七 feature 最小子集与七个离线 tool crate 均不引入 `reqwest|hyper|rustls|h2|tower`（离线 tool crate：`synthia-tool-read` / `-write` / `-shell` / `-todo` / `-task` / `-scheduler` / `-search`）；synthia-search 由专段断言（default + `provider` 两种配置）；`synthia-tool-scheduler` 同样由专段断言（default + `--features cron` 两种配置），且其后紧跟 cron 表面的 rustdoc 检查（`RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p synthia-scheduler --features cron`）——`doc-check` 只建默认 features，没有这一行则 cron 门后的文档在整个 `make ci` 里从不被检查。
- 异步运行时同样按 crate 声明，禁止从 workspace 继承 `tokio` 的 `full`：
  - 每个 crate 的 `tokio` 行只写自己代码真正用到的 feature（例：`synthia-provider` 用 `["macros", "time"]`，`synthia-session` 用 `["rt", "sync", "time", "macros"]`），并附一行原因注释。
  - 仅在测试中使用 tokio 的 crate（`synthia-core` / `context` / `steering` / `skill` / `eval` / `macros` / `workflow` / `synthia-tool-scheduler` / `synthia-tool-search`）一律放 `[dev-dependencies]`。
  - 运行时的唯一"脱离运行时"接缝是 `synthia_core::spawn::Spawner`（`ReActAgent::with_spawner` / `ToolRegistry::with_spawner`）；新增需要 spawn 的代码必须走该 trait，不得直接 `tokio::spawn`。
  - `make check-no-runtime` 保证 `synthia-core` / `synthia-scheduler` / `synthia-macros` / `synthia-eval` / `synthia-workflow` / `synthia-search` 与 `synthia-telemetry --no-default-features` 的 lib 构建不含 tokio。
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
- 必须满足 workspace 声明的 MSRV `rust-version = "1.98"`；引入依赖前确认其 MSRV 不高于本项目。

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
- [scripts/harness-shape/clippy.toml](file:///home/crochee/workspace/synthia/scripts/harness-shape/clippy.toml) — **形状预算** `too-many-lines-threshold = 100` 与 `excessive-nesting-threshold = 4`。它**故意不放在根 `clippy.toml`**：把这两个键写进根配置会**全局启用**这两个默认关闭的 lint（`excessive_nesting` 的默认值 0 即禁用），从而让 `make lint-rust -D warnings` 在本轮未改动的 crate 上报错。该文件只由 `scripts/check-harness-shape.sh` 通过 `CLIPPY_CONF_DIR` 注入，是这两个阈值的**唯一真源**。
- [Cargo.toml](file:///home/crochee/workspace/synthia/Cargo.toml) — workspace `resolver = "2"`、`edition = "2024"`、`rust-version = "1.98"`；release profile 已固定 `opt-level = 3` / `lto = "thin"` / `codegen-units = 1` / `strip = "symbols"`，**不得随意调整**。
- [deny.toml](file:///home/crochee/workspace/synthia/deny.toml) — `cargo-deny` 的许可证 / 来源 / ban 审计入口（已落地的最低策略：bans/licenses/sources 三项在 `make check-deny` 跑通；advisories 由 `cargo-audit` 单独负责，见 `.github/workflows/security-audit.yml`）。新增依赖须保证通过该审计。
- [.markdownlint.json](file:///home/crochee/workspace/synthia/.markdownlint.json) — markdown lint 锁，作用于 26 个 crate 顶层 `README.md` 与 `CONTRIBUTORS.md`（与 `make fmt-check` 互不重叠；CI 入口 `.github/workflows/markdown-lint.yml`）。
- [.editorconfig](file:///home/crochee/workspace/synthia/.editorconfig) — 编辑期 LF / UTF-8 / `trim_trailing_whitespace` / `final_newline` / Makefile `indent_style = tab`。**与 rustfmt / prettier 互补而非重复**：只覆盖它们不触达的文件类型（shell / TOML / Markdown 等），不与 formatter 抢同一文件的格式规则。
- [.gitattributes](file:///home/crochee/workspace/synthia/.gitattributes) — git 期 LF 强制 + binary 标记 + linguist 关闭 vendor/generated + `CHANGELOG.md` / `Cargo.lock` 用 `merge = union` 防冲突 + `Cargo.lock` 不进 diff。
- `[workspace.lints]` in [Cargo.toml](file:///home/crochee/workspace/synthia/Cargo.toml) — workspace 级 rustdoc 棘轮：`missing_docs = warn` / `broken_intra_doc_links = warn` / `private_doc_tests = warn` / `missing_crate_level_docs = warn`（Cargo 1.74+ 的单一收口，所有成员 crate 自动继承，除非该 crate 在自己的 `[lints]` 表覆盖）。基线已实测 0 命中（`cargo clippy --all-targets --all-features --tests --all -- -D warnings` + `cargo doc --no-deps --workspace` 双绿）。`warn` 而非 `deny` 是给历史留余地；某天升 `deny` 时本条 + `[workspace.lints]` 同步更新即可。
- `[workspace.metadata.docs.rs]` in [Cargo.toml](file:///home/crochee/workspace/synthia/Cargo.toml) — 驱动 `docs.rs/<crate>` 渲染：默认渲染是"default features only"，会让 `cron` / `mcp` / `search` / `eval` / `workflow` / `scheduler` / `telemetry` / `sqlite` / `attachment` / `skill` / `tool-scheduler` / `tool-search` 等 opt-in 表面渲染为空模块。本仓显式声明 `all-features = true` + `targets = ["x86_64-unknown-linux-gnu"]`，让 docs.rs 渲染完整 opt-in 契约（与 `check-mvp-deps` 已证明的 7-feature 最小子集零 HTTP/OTel/DB 依赖兼容）。
- [CLAUDE.md](file:///home/crochee/workspace/synthia/CLAUDE.md) — Anthropic Claude Code 标准的 agent 行为守则（"Think before coding / Simplicity first / …"），138 行；与 [AGENTS.md](file:///home/crochee/workspace/synthia/AGENTS.md) 配套——AGENTS.md 给"做什么 / 不做什么"，CLAUDE.md 给"怎么做"（思考优先级、避免常见 LLM 编码错误、推到完成）。修改 CLAUDE.md 必须知道它不是项目特定规则，而是与 Anthropic 上游共识对齐的 agent 工作流；与本仓约定的冲突部分以本仓为准。

## 3.5 圈复杂度与代码异味

- **形状预算才是被强制的那条**：`synthia-harness` 生产代码不得有函数超过 100 行或 4 层嵌套，由 `make check-harness-shape` 断言（阈值在 [scripts/harness-shape/clippy.toml](file:///home/crochee/workspace/synthia/scripts/harness-shape/clippy.toml)，由 `scripts/check-harness-shape.sh` 现读，不会漂移）。超出时优先拆分而非放宽阈值。
- `cognitive-complexity-threshold = 20` 保留但**不再作为拆分理由**：该 lint 属 `restriction` 组（默认关闭），且按其自身文档已不再度量"认知复杂度"——只数决策点，不计嵌套 / 循环 / `?` / 闭包嵌套。R122 实测整个 `synthia-harness` 循环上限 8/20，所以旧的「为满足复杂度阈值而拆分」说法是无据的。要度量真实结构请用上面两个 lint。
- 禁止使用通配符导入（`use foo::*;`），由 `warn-on-all-wildcard-imports = true` 强制。
- 优先使用 `for` 循环处理副作用场景，而非 `for_each` / `try_for_each`。
- 优先 `map(..).unwrap_or(..)` / `map(..).unwrap_or_else(..)`，而非 `map_or` / `map_or_else`。
- 禁止 `std::mem::forget`（内存泄漏风险）与 `std::ptr::read_unaligned`（unsafe 指针操作）。
- 编写 doc comment 时遵守 `doc-valid-idents` 白名单（如 `MiB`、`IPv4`、`OAuth`、`PostgreSQL` 等），避免触发 `clippy::doc_markdown` 警告。

## 3.6 自动化入口（Makefile）

优先通过仓库根目录的 [Makefile](file:///home/crochee/workspace/synthia/Makefile) 触发质量门禁，避免在脚本里拼装裸命令：

> **例外：需要下沉到脚本的门禁**。两种情况下门禁由**脚本**承载，而不是内联 recipe：
>
> 1. **需要只对它生效的工具配置**（`check-harness-shape` 属于此类：把
>    `too-many-lines-threshold` / `excessive-nesting-threshold` 写进根 `clippy.toml` 会**全局启用**这两个默认关闭的
>    lint，从而让 `make lint-rust -D warnings` 在本轮未改动的 crate 上报错）。此时门禁是**两个文件**，两者
>    **不必同名**——本仓库的实例就是这样，照抄时注意区分：
>
>    | 文件 | 作用 | 实例 |
>    |---|---|---|
>    | `scripts/<gate>.sh` | 门禁脚本本体（recipe 只有一行 `@bash scripts/<gate>.sh`） | `scripts/check-harness-shape.sh` |
>    | `scripts/<gate-dir>/clippy.toml` | 只对该门禁生效的工具配置，由脚本通过 `CLIPPY_CONF_DIR` 注入 | `scripts/harness-shape/clippy.toml` |
>
> 2. **判定逻辑超出单行 recipe 的表达力**（`check-public-api-runtime` 属于此类：要现读每个文件对运行时 crate 的
>    `use` 别名，再跨签名续行扫描 `pub` 项，一行 grep 只能匹配「`pub` 与 `tokio::` 同行」这种最弱的泄漏形态）。
>    这类门禁**只有一个文件**——没有配套配置目录：
>
>    | 文件 | 作用 | 实例 |
>    |---|---|---|
>    | `scripts/<gate>.sh` | 门禁脚本本体 | `scripts/check-public-api-runtime.sh` |
>
> 脚本**不在**配置目录里（`scripts/<gate>/…` 是错的——recipe 会找不到它）。**判定标准**：只有当门禁需要隔离的
> 工具配置、或多步控制流在 make recipe 里已被证明易碎（续行 / 变量作用域 / 需要解析别名）时才允许下沉到脚本；
> 其余门禁一律内联在 Makefile 里。

| 目标               | 作用                                                                  |
| ---------------- | ------------------------------------------------------------------- |
| `make fmt-rust`  | 格式化 Rust 代码（`cargo +nightly fmt --all`）                             |
| `make fmt-check` | 校验格式（`cargo +nightly fmt --all --check`）                             |
| `make lint-rust` | Clippy（`--all-targets --all-features --tests --all -- -D warnings`） |
| `make test-unit` | 逐个 crate 运行库单元测试（`cargo test -p <crate> --lib`，CI 友好）              |
| `make test-crates` | 逐个 crate 运行完整测试（禁止 `cargo test --workspace`，见 §3.3）             |
| `make test-sqlite` | 运行可选 SQLite 记忆层测试（`cargo test -p synthia-context --features sqlite`） |
| `make examples`  | 运行全部 example 与两个独立 consumer crate（各自打印 proof line）               |
| `make check-mvp-deps` | 断言七 feature 最小子集与七个离线 tool crate 不引入 HTTP/OTel/DB 依赖，且 `--no-default-features` 可编译 ；synthia-search 专段断言 default+provider 两种配置；synthia-tool-scheduler 专段断言 default+`--features cron`，并检查 cron 表面 rustdoc 无警告 |
| `make check-no-runtime` | 断言 runtime-free crate 的 lib 构建不含 tokio                          |
| `make check-public-api-runtime` | 断言库 crate 的**公开 API** 不出现运行时类型（`synthia-server` 与 `synthia-mcp-server` 豁免，二者皆为应用 / 二进制 crate，`main()` 是唯一允许出现具体运行时类型的地方；R125 同步修复了多行 `use` brace body 未注册的盲区）：现读每个文件对 `tokio` / `tokio_util` / `tokio_stream` / `async_std` / `smol` / `futures::executor` 的 `use` 别名，再跨签名续行扫描 `pub` 项，因此 `use tokio::sync::mpsc;` + `pub fn new(tx: Arc<mpsc::UnboundedSender<…>>)` 这类别名泄漏也会被抓到（见 [scripts/check-public-api-runtime.sh](file:///home/crochee/workspace/synthia/scripts/check-public-api-runtime.sh)） |
| `make check-test-layout` | 棘轮：生产文件内联 `#[cfg(test)] mod` 测试块不得 ≥400 行（硬上限），300 行档位已清零（基线 0）；新增测试放同级 `tests.rs` |
| `make check-pub-surface` | 棘轮：`crates/*/src` 内禁止 `pub use <内部模块>::*;`（facade 传播 `synthia_xxx::*` 除外） |
| `make check-harness-shape` | 断言 `synthia-harness` **生产代码**无函数超过 100 行或嵌套超过 4 层（`too_many_lines` + `excessive_nesting`，阈值现读 [scripts/harness-shape/clippy.toml](file:///home/crochee/workspace/synthia/scripts/harness-shape/clippy.toml)；见 [scripts/check-harness-shape.sh](file:///home/crochee/workspace/synthia/scripts/check-harness-shape.sh)，仅限该 crate） |
| `make test-guides` | 编译 `MINIMAL.md` 的 `rust` 围栏（doctest）：先默认 feature，再七 feature MVP 子集。`doc-check` 只**生成**文档、不**编译**示例，所以向导里的配方只能靠这个目标（和 CI）拦住 |
| `make test-layout-audit` | 打印当前所有 ≥300 行的内联测试块位置，便于核对棘轮                        |
| `make check-claim-language` | 棘轮：`README.md` / `MINIMAL.md` / `SEAMS.md` / `CHANGELOG.md` [Unreleased] / `crates/*/README.md` / facade 与范式 lib crate 的 rustdoc 里若出现 "paradigm only" / "zero tool implementations" / "default tool set" / "ships no implementations" / "the built-ins" 等与代码事实矛盾的绝对化措辞即失败；冻结的 `docs/optimization-report-R*.md` / `docs/superpowers/` / 已发布 CHANGELOG 条目豁免（与本文件 §3.7 配套） |
| `make check-clock` | 棘轮：库 crate 生产代码不得有 `chrono::Utc::now()` 直接调用（基线 0；app crate `synthia-server` 豁免，`#[cfg(test)]` 豁免）——一律走 `synthia_core::Clock`（§3.8） |
| `make check-deny` | 跑 `cargo deny check licenses bans sources`（公告检查 `advisories` 由 `.github/workflows/security-audit.yml` 单独负责）；新增依赖必须保证通过 |
| `make check-web` | 前端门禁：`tsc --noEmit` + `eslint` + `prettier --check`；本项目**不**留前端 e2e 套件（详见 §4.2） |
| `make lint-sh` | 对 `scripts/check-harness-shape.sh` 与 `scripts/check-public-api-runtime.sh` 跑 shellcheck（`SC1091`/`SC2086` 豁免，`-S warning`）——`.github/workflows/shellcheck.yml` 是 CI 对应入口 |
| `make ci`        | 快速门禁 = `fmt-check` + `lint-rust` + doc-check + test-guides + 依赖/布局/形状/时钟/许可证不变量目标            |

CI（[.github/workflows/rust-quality.yml](.github/workflows/rust-quality.yml)）只调用上述 `make` 目标，分 `gates` / `tests` / `examples` / `web` / `bench` 五个 job（`cargo fetch` / `npm ci` 属依赖安装，不是门禁）；测试与示例均无需 API key。

除 `rust-quality.yml` 外，本仓还在以下 workflow 跑自动化门禁（任一红都不可合）：

| Workflow | 触发 | 入口 |
| :--- | :--- | :--- |
| `ci.yml` | push / PR | CNCF 范式快环：fmt + clippy `-D warnings` + MSRV + 跨平台 build smoke |
| `docs.yml` | push | `cargo doc --all-features` + 部署到 `gh-pages` |
| `release.yml` | `v*` tag | 多平台 release 产物 + SHA-256 + CycloneDX SBOM + SLSA Build L3 provenance |
| `security-audit.yml` | push / PR + 周一 03:00 UTC | `cargo audit`（rustsec/advisory-db）+ `cargo deny check` |
| `contract-closure.yml` | push / PR | 双侧接口契约闭环扫描（advisory；§6 闭环时升级为 gating） |
| `dependabot-auto-merge.yml` | Dependabot PR | patch/minor 自动合（major 走人审） |
| `markdown-lint.yml` | push / PR + 每天 UTC 04:00 | `markdownlint-cli2` 跑 26 个 crate README + `CONTRIBUTORS.md` |
| `shellcheck.yml` | push / PR + 每天 UTC 05:00 | `shellcheck` 跑 `scripts/check-harness-shape.sh` + `scripts/check-public-api-runtime.sh` |
| `stale.yml` | 每天 UTC 02:00 | CNCF 范式：停滞 issue / PR 自动 stale → close（详见 `.github/stale.yml`） |

任何对 `clippy.toml` / `scripts/harness-shape/clippy.toml` / `rustfmt.toml` / `deny.toml` / `Cargo.toml`（含 `[workspace.lints]` 与 `[workspace.metadata.docs.rs]`） / `.markdownlint.json` / `.editorconfig` / `.gitattributes` / `.github/stale.yml` / `.github/workflows/*.yml` / `CLAUDE.md` 的修改必须在同一变更中更新本文件对应引用，避免规则与配置漂移。

# 3.7 架构原则：可作为 lib + 乐高式组装

合成 Synthia 项目的两个最高级目标，决定下面的所有具体规范：

1. **可作为 lib** — `cargo add synthia` 是消费端的入口；下游只依赖
   `synthia` / `synthia-*` crates，绝不能反向依赖消费端应用。
2. **乐高式组装** — 七大组件（Provider / Tool / ContextManager /
   Steering / SessionSink / CancelToken / Agent）每一个都是
   单独可换的 trait + 默认实现；外部消费者应能从一个空的
   `cargo new` 仓库拼出一个完整的 AI agent。

由此衍生以下硬性约束：

- **不依赖具体运行时**：除 `synthia-server`（应用 crate，可选 feature）
  与 `synthia-mcp-server`（stdio 二进制，R125）外，任何 crate 都不应在
  公共 API 暴露 `tokio::*` / `async-std::*` / `smol::*` 类型。
  取消用 [`synthia_core::CancelToken`] trait，
  流用 `futures::Stream`，事件通道用 `futures::channel::mpsc`；
  分离任务（detach）一律走
  [`synthia_core::spawn::Spawner`](crates/synthia-core/src/spawn.rs)
  （`ReActAgent::with_spawner` / `ToolRegistry::with_spawner`），
  禁止直接 `tokio::spawn`；各 crate 的 tokio feature 按需声明（见 §1）。
  可验证证据：`cargo run --example runtime_agnostic -p synthia-harness`
  （非 tokio 执行器跑完整一轮）与 `make check-no-runtime`。

  **`synthia-mcp-server` 的 I/O 接缝以 trait 形式公开**：
  其 lib 的 `run<R, W>` 是泛型函数，`R: tokio::io::AsyncRead + Unpin`
  与 `W: tokio::io::AsyncWrite + Unpin` —— trait 是 **trait bound**
  而非具体类型。`scripts/check-public-api-runtime.sh` 仍按 §3.7
  的"零 `tokio::` 路径"规则捕获它（trait bound 也算路径出现），
  所以整个 crate 走 `synthia-server` 同款豁免（应用 / 二进制 crate
  的 `main()` 是唯一允许出现 `tokio::io::stdin()` / `stdout()`
  等具体类型的地方）。`make check-public-api-runtime` 把
  `synthia-server` 和 `synthia-mcp-server` 都列入豁免。
- **chrono 是唯一的时间库**：墙钟时间一律走
  [`synthia_core::Clock`](crates/synthia-core/src/clock.rs)
  trait（默认实现 [`SystemClock`](crates/synthia-core/src/clock.rs)），
  单调时间用 `std::time::Instant` / `std::time::Duration`。
  不要直接调用 `chrono::Utc::now()`；需要测试时序的代码用
  [`FixedClock`](crates/synthia-core/src/clock.rs) 注入。
- **ULID 是默认 ID 生成器**：通过
  [`synthia_core::IdGen`](crates/synthia-core/src/idgen.rs) trait，
  默认实现 [`UlidGenerator`](crates/synthia-core/src/idgen.rs)，
  测试用 [`SequenceGenerator`](crates/synthia-core/src/idgen.rs)。
- **零逻辑 facade**：[`crates/synthia/src/lib.rs`](crates/synthia/src/lib.rs)
  没有 `fn` / `impl`，仅做 `pub use`。所有逻辑归属于底层
  `synthia-*` crate。
- **精选 re-export，单一真源在底层 crate**：每个 `synthia-*` crate 的
  `lib.rs` 是决定"哪些名字是公开的"的**唯一**地方——用显式 `pub use
  module::{A, B};` 列名，**禁止** `pub use module::*;` 把某个内部模块的
  public 项整体倒出（那会让任何新增 `pub` 自动变成对外契约）。facade
  (`crates/synthia/src/*.rs`) 只做 `pub use synthia_xxx::*;` 的传播，
  因此 facade 的公开面 = 底层 crate 的精选面，二者不会漂移。
  门禁：`make check-pub-surface`（`crates/*/src` 内出现
  `pub use <非 synthia_*>::*;` 即失败）。
- **公开面最小化**：只在 crate 外被实际使用的项才 `pub`；其余 helper /
  sentinel / 内部 type alias 必须 `pub(crate)` 或 `pub(super)`。判断标准：
  在被改 crate 的 `Cargo.toml` 之外的任何 `.rs`（含 examples、tests、
  doc-tests、facade、其它底层 crate）里出现 `crate::path::name` 的引用，
  才算"外部使用"（**必须实际 grep 验证，不能凭印象**——本轮就出现过
  「以为只有本 crate 用、实际被 `synthia-tool` 跨 crate 引用」的例子）。
- **禁止范式：`<module>.rs` + 同名 `<module>/` 子目录并存**。每个模块只有一种组织：要么 `<module>.rs`（实现 + 内联 `#[cfg(test)] mod tests`），要么 `<module>/mod.rs` + 子模块文件 + `<module>/tests.rs` 或 `tests/`。一旦某模块测试超过 400 行（`check-test-layout` 硬上限），外迁到同目录兄弟 `tests.rs`（由 `lib.rs` 的 `#[cfg(test)] mod tests;` 声明），**不要**把测试挪到同名子目录——同名子目录会让 reader 误以为里面是子模块。
- **大模块（>300 行）保留 mod.rs 范式**：拆出 `<module>/mod.rs` + 多个子 `.rs` 时，**测试仍走内联 `#[cfg(test)] mod tests`**（在 `mod.rs` 里），不要给子 `.rs 各自建 `tests/` 子目录。子文件互相紧密耦合，测试也应在一个文件里方便看。
- **依赖方向单向**：`synthia` → `synthia-*` → `synthia-core`，
  一一对应，并被同名 feature gate。新增 crate 必须同步加
  feature；删除 module 必须同步删 feature。
- **工具实现 = 独立 plugin crate，范式留在 `synthia-tool`**：
  `synthia-tool` 只承载范式（`Tool` trait、`ToolRegistry`、曝光投影、
  `workspace` 路径约束）与两个合成契约工具
  （`RetrieveFullOutputTool`、`StructuredOutputTool`），
  **不含面向 agent 的内建工具集**。每个 agent 可见工具一个 crate
  （`synthia-tool-read` / `-write` / `-shell` / `-todo` / `-web` /
  `-task` / `-scheduler` / `-search`），消费方按需 `registry.register_entry` 自行组装，
  不存在隐式的"默认工具集"。纯工具插件（`read` / `write` / `shell` /
  `todo` / `web`）只依赖范式 crate；三个例外——`-task` 走 harness 的
  `ToolInterceptor` seam，因此额外依赖 `synthia-harness`；`-scheduler`
  把 `synthia-scheduler` 的 `ScheduleStore` 适配成 `schedule` 工具，
  因此依赖 `synthia-scheduler`（见下一条）；`-search` 把宿主构建的
  `synthia-search::Registry` 适配成跨域 `search` 工具，因此依赖
  `synthia-search`（同样离线，见下部 `synthia-tool-search` 条目）。
  **工具自己的边界随工具走**：一个进程级策略只有跑进程的工具才需要，
  因此它属于该插件而不是范式 crate —— `ExecutionPolicy` /
  `SandboxBackend` / `BwrapBackend` / `effective_policy` 等 OS 执行策略
  位于 `synthia-tool-shell::sandbox`，由 `ShellTool` 自己持有
  （`with_policy` / `with_sandbox`），所以"告诉模型的描述"与"真正 spawn
  的 argv"来自同一个值，不可能漂移。范式 crate 的 `Context` 是**每个**
  工具共用的，因此不放任何单工具的策略字段（放了就等于让一个工具的设置
  悄悄改变另一个工具的行为）。
- **harness 与插件分离**：`synthia-harness` 只保留核心循环 + registry
  组装 + 类型定义；需要循环内部能力（事件转发、取消、深度）的合成工具
  走 [`synthia_harness::ToolInterceptor`](crates/synthia-harness/src/agent/interceptor.rs)
  seam（`with_interceptor` / `ReActAgent::with_interceptor`）。多智能体委派
  是这条 seam 的第一个实现，整体位于 `synthia-tool-task`（`task` 工具、
  gate、worktree 隔离、`@agent:` router、subagent 池）。新增同类合成工具
  一律新建 plugin crate 实现该 trait，**不得**回填进 `synthia-harness`。
- **依赖方向单向**：`synthia` → `synthia-*` → `synthia-core`，
  永远不回指；用 `cargo tree -p synthia-core` 验证无环。
- **Registry seam 收口在 `synthia-core`**：所有"具名目录 + cursor 分页"
  的抽象（[`synthia_core::registry::Registry`](crates/synthia-core/src/registry.rs)
  + [`RegistryItem`](crates/synthia-core/src/registry.rs) +
  [`paginate_registry_list`](crates/synthia-core/src/registry.rs) 辅助函数）
  都从 `synthia-core` 出发。[`AgentRegistry`](crates/synthia-harness/src/agent/registry/mod.rs)
  通过这套 trait 完整暴露；[`ToolRegistry`](crates/synthia-tool/src/registry/mod.rs)
  在私有 `mod registry_trait` 子模块里以同套语义实现（不公开 `ToolFilter` /
  `ToolEntry` 这些内部类型，外层仍可用固有方法 `register_entry` /
  `descriptors_cached` 达到同样效果）。
  新增具名目录（servers / channels / runbooks …）一律走同一条 seam，
  不要在自己的 `mod` 里再写一份 cursor + limit + envelope 的拷贝。
- **Optional crates are not bloat, they are opt-in extensions**：七个
  `synthia-scheduler` / `synthia-workflow` / `synthia-search` /
  `synthia-skill` / `synthia-mcp` / `synthia-eval` /
  `synthia-attachment` — 每个都拥有自己的 lib 测试（≥14 个用例）、
  至少一个 example，并由 facade 上同名 feature 单独 gate。
  默认 `synthia = "0.1"` 不会拉起其中任何一个；`make check-mvp-deps`
  断言这一点。要决定"是否删除一个 crate"，先回答三个问题：
  (1) 它有没有自带测试（`cargo test -p <crate> --lib` 通过）？
  (2) 它在 facade 上有没有 feature？
  (3) 是否有 example 在 reference 用它？三者任一为"否"再考虑删除。
  当前所有可选 crate 都满足三个"是"，属于活跃维护范畴。
- **新增二进制 crate `synthia-mcp-server`**（R125，与上节七
  个可选 crate **并列**但分类不同 — 它们是 lib，本 crate 是
  单一可执行）：stdio 上的 MCP 服务器，把 `read` / `write` /
  `shell` / `TodoWrite` / `web_fetch` / `schedule` / `search` /
  `synthia` 这 8 个 plugin-brick 工具暴露给任意外部 MCP 客户端
  （`pi`、Claude Code 插件、Anthropic SDK …）。任何想消费
  synthia 工具面的下游应用 `spawn("synthia-mcp-server",
  ["--workspace", cwd])` 即可在自己的 JSON-RPC 2.0 栈下拿到
  完整工具集。`lib.rs` 的 `run<R, W>` 泛型函数以 `tokio::io
  ::AsyncRead + Unpin` / `AsyncWrite + Unpin` 作为 I/O 接缝
  （任何 runtime 都满足），`main.rs` 是唯一持有具体运行时
  类型的地方（与 `synthia-server` 同款 §3.7 豁免）。
  验证：`crates/synthia-mcp-server/tests/stdio_roundtrip.rs`
  5/5（spawn binary 后驱动完整 wire）+ `crates/synthia-mcp-server/
  tests/pi-integration.ts`（Node.js 跨语言 smoke，
  `OK: cross-language smoke passed`）。
- **新增 crate `synthia-search`**（R115）：通用多域搜索引擎。
  BM25 + 向量混合 / 可插拔 `Tokenizer` / `Embedder` / `VectorStore`
  / `Filter<T>` / `Reranker<T>`；增量 add / tombstone remove /
  阈值触发 compact；`parking_lot::RwLock` 保证线程安全，无
  `LockPoisoned`；`Registry` 以**域标签**为键跨 `T` 检索
  （`register` 沿用 `std::any::type_name::<T>()`，`register_domain`
  / `register_erased` 由宿主给定 `"tool"` / `"skill"` … 友好标签，
  `Hit::domain` 由 `Registry::search` 盖章，`QueryContext::domains`
  过滤参与合并的引擎）；`ErasedEngine::search_erased` 与
  `Registry::search` 是 async，异步检索器（会话全文索引、memory
  recall）因此不必在 tokio worker 里做阻塞桥接；`Searchable: RegistryItem`
  复用了 `synthia-core::RegistryItem`
  词汇表（与现有 `Document` / `Skill` 平级，无平行类型体系）。
  默认 feature 零外部依赖（`HashingEmbedder` 主线、确定性、CPU-only
  ，dev/test 友好）；可选 `provider` feature 拉起
  `synthia-provider`（`default-features = false`，**不**拉 reqwest）
  启用 `ModelProviderEmbedder` 适配器。facade 上以 `search` feature
  暴露（默认 NO），不在默认集合。门禁：`check-mvp-deps` 为
  synthia-search 增加专段，断言 **default 与 `--features provider`
  两种配置**均不引入 `reqwest|hyper|rustls|h2|tower`
  （`synthia-provider` 以 `default-features = false` 拉入，不带
  HTTP adapter）；`check-no-runtime` 断言 **default** 配置的 lib
  构建不含 tokio——开 `provider` 后经由 `synthia-provider` 的无条件
  tokio 依赖（`macros`/`time`）引入 tokio，属预期（该 feature 本就
  面向自带运行时的调用方）。详见
  `docs/superpowers/specs/2026-09-19-synthia-search-design.md`。
- **新增 crate `synthia-tool-search`**（R117，facade feature
  `tool-search`）：把宿主构建的 `synthia-search::Registry` 适配成模型
  可调的跨域 `search` 工具——`SearchTool::call` 直接复用
  `synthia_search::search_tool_ctx` 的 agent-view 投影（`domain` /
  `id` / `title` / `why` / `score`，域内带内容时附 `preview`），
  可选参数 `domain` 收窄到一个目录；`register_search_tool` 以
  `ToolExposure::Deferred` 落位（冷启动只广告 name + description 与
  占位 schema，首次 `search` 调用后升级为真实的 `query` / `limit` /
  `domain` schema）。离线（依赖 `synthia-core` / `synthia-tool` /
  `synthia-search`，无 HTTP），属于 `-task` / `-scheduler` 同款
  "插件依赖非范式 crate" 的例外；门禁：`check-mvp-deps` 的
  `OFFLINE_TOOL_CRATES` 已含本 crate（正常依赖不含 tokio，tokio 仅出现在
  `[dev-dependencies]`）。example：
  `cargo run --example search_tool_demo -p synthia-tool-search`
  （尾行 `SEARCH-TOOL: OK`）。Spec：
  `docs/superpowers/specs/2026-09-19-wheel-cron-search-persistence-design.md` §3。
- **synthia-server 的六域封装**（`crates/synthia-server/src/search/`）：
  服务器层把 tool / mcp / skill / memory / agent / session 六个目录
  投影成 `synthia-search` 的引擎（`SearchDoc` + `CatalogEngine`
  自刷新快照；memory / session 走 `ErasedEngine` 适配器直接委托各自
  检索）。一个 `SearchService` 提供两个视图：**agent 视图**（五域，
  不含 session）经 `register_search_tool` 成为模型可调的 `search`
  工具，所以冷启动上下文只需广告工具目录而不必全量枚举；
  **HTTP 视图**（六域）由 `GET /api/v1/search` 投影，前端各资源的搜索
  都走这一个引擎。用户/租户语义只存在于服务器层：HTTP handler 把
  `RequestUserId` 放进 `QueryContext::extra`，session 域据此限域；
  agent 视图不暴露会话正文（模型调用没有可限域的请求上下文）。
  接线后 MCP 工具默认降到 `ToolExposure::Deferred`（per-request
  schema 体积变小，调用一次即升级）。
- **Claim-language gate**：当前状态文档（`README.md` / `AGENTS.md` /
  `MINIMAL.md` / `SEAMS.md` / `CHANGELOG.md` 的 `[Unreleased]` 段 /
  `crates/*/README.md` / facade 的 `src/lib.rs` 与 `src/tool.rs` /
  `synthia-tool/src/lib.rs`）禁止与代码事实相矛盾的「绝对化措辞」
  —— `paradigm only` / `zero tool implementations` /
  `ships no implementations` / `default tool set` /
  `default registry` / `the built-ins` / `four offline builtins` 等。
  这一规则的来源：本项目在 plugin split（2026-09-16）前后多次
  发现「paradigm only / no default set」类的绝对化表述与
  真实代码（范式 crate 仍携带 `RetrieveFullOutputTool` /
  `StructuredOutputTool` 与 `ToolEntry::dynamic` 的直通）相矛盾；
  `make check-claim-language` 是把这些表述棘轮化、自动拦截同
  类问题的尝试。可验证证据：`make check-claim-language`（当
  前状态应返回 `OK: no absolute claim-language in current-state docs`）。
  冻结历史文件（`docs/optimization-report-R*.md`、
  `docs/traitclaw-gap-analysis.md`、`docs/architecture/adr/`、
  `docs/superpowers/`、已发布的 CHANGELOG 条目）默认豁免——
  它们是过去的轮次记录，按 R46 报告确立的先例保持原样。

## 3.8 时间与 ID 的使用准则

- **生产代码**：`SharedClock::system()` 构造一次，传 `Arc<SharedClock>`
  给所有需要的组件（`synthia-server` 的 `AppState` 就是这一份时钟的下游）。
  `SharedIdGen::ulid()` 或 `SharedIdGen::ulid_prefixed("tag")` 同理。
- **测试代码**：`SharedClock::fixed_at(t)` /
  `SharedClock::fixed_from_rfc3339(s)` 注入固定时间；
  `SharedIdGen::sequence()` 注入确定性 ID 序列。
- **新代码**禁止直接写 `chrono::Utc::now()` 或
  `ulid::Generator::new().generate()` — 必须经过 trait，方便测试与替换。
  需要"当下"的公开构造函数一律提供 `_at(..., now)` 变体
  （例：`MemoryEntry::at`、`ImageAttachmentRef::from_bytes_at`、
  `OperationSnapshot::new_at`、`JournalEntry::success_at`、
  `parse_retry_after_at`），默认形式再走 `SystemClock`。
- **门禁**：`make check-clock` 对 **lib crate** 生产代码里
  `chrono::Utc::now()` 的出现次数做棘轮。`synthia-server` 是 **app**
  crate（§3.1：libs 用 thiserror，apps 用 anyhow），应用层直接读墙钟
  合规；`#[cfg(test)]` 块也豁免（测试不走 trait 注入的固定时钟）。当
  前基线 **0**（排除注释、`#[cfg(test)]`、app crate 后的 lib 生产调用）；
  `make clock-audit` 打印每一处，便于核对；计数增长即失败。

***
***

# 4. Web 前端编码规范

## 4.1 基本要求

- 0 lint（无 lint 错误/警告）。
- 需要格式化。
- 能运行起来（`npm run build` 通过）。
- 代码符合 Web TS / HTML / CSS 编码规范。
- 门禁：`make check-web`（`tsc --noEmit` + `eslint` + `prettier --check`，无需浏览器）。

## 4.2 前端验证：Playwright MCP，不写 spec

**本项目不保留任何前端自动化测试套件，验证一律用 Playwright MCP 直接驱动浏览器。**

1. 起真实前端（`npm run dev`，必要时 `cargo run -p synthia-server`）。
2. 用 MCP 打开页面、执行交互、读取 DOM 与真实渲染结果作为证据。
3. 纯函数同样如此：Vite 把 `src/**` 作为模块直接提供，页面内
   `await import('/src/lib/foo.ts')` 即可断言，无需测试 harness。

   **MCP 的两个易踩陷阱**（必须处理，否则 chat 页会"看起来没反应"）：

   - **后台 tab 的 `visibilityState === 'hidden'`**：`useServerHealth` 的
     `probe()` 在第一次检查时就 early-return，把整个 app 标成 Offline，
     输入框与 send 按钮置灰，无任何错误信息。打开 tab 后在 navigate 前
     先 patch：

     ```js
     await page.evaluateOnNewDocument(() => {
       Object.defineProperty(document, 'visibilityState',
         { get: () => 'visible', configurable: true });
       Object.defineProperty(document, 'hidden',
         { get: () => false, configurable: true });
     });
     await page.bringToFront();
     ```

     或更直接地 mock `/readyz` 200 + `bringToFront`，效果一样。
   - **隐藏 tab 里 `requestAnimationFrame` 永不触发**：`page.evaluate`
     里 `await new Promise(r => requestAnimationFrame(r))` 会一路挂到
     `protocolTimeout`（默认 30 s）。验证 `naturalWidth` 之类的渲染后
     量应改用 `setTimeout` 或 MCP 提供的 `wait(...)` 助手，否则就是
     30 秒超时 + 一个莫名其妙的失败。

理由：

- 前端 e2e 套件（原 44 个 spec / ~5,200 行）跑一次 **13 分钟**，且需
  `cargo build` + Vite + Chromium 三件套，成本远高于它发现的问题。
- spec 与实现会同步腐化：曾出现整批 mock 停留在重构前的 wire 形状、
  长期红灯无人察觉（`continue-chat` / `session-detail` / `is-tool-echo-log` 等）。
- MCP 打在真实应用上，不引入第二份需要维护的契约。MCP 的浏览器是
  harness 自带的，与 `synthia-web` 的依赖无关（已在移除 `playwright`
  依赖后实测通过）。

因此：`synthia-web/tests/`、`playwright*.config.ts`、`.github/workflows/e2e.yml`
与 `synthia-web/scripts/e2e/` 均已删除；新增前端验证不要再建这些。
