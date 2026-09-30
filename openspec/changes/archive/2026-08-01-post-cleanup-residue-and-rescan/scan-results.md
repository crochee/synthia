# Lightweight Rescan Results — 2026-08-01

执行时间：2026-08-01（Sat）— 配合 OpenSpec 变更 `2026-08-01-post-cleanup-residue-and-rescan`
执行人：Synthia agent（在 master 分支，未推送）

## 1. `cargo +nightly clippy --all-targets --all-features --tests --all`

- **状态**: PASS, exit 0
- **耗时**: 2m 52s
- **警告数**: 0
- **错误数**: 0
- **涵盖**: 16 个 workspace member + test-support + 所有 features
- **最后一行输出**: `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 2m 52s`
- **结论**: clippy 干净，**本提案无 clippy 回归风险**

## 2. `cargo +nightly fmt --all -- --check`

- **状态**: PASS, exit 0
- **stdout**: 空（即已对齐 `rustfmt.toml` 规范）
- **结论**: 格式已对齐

## 3. `cargo +nightly fmt --all`（同步步）

- **状态**: PASS, exit 0
- **diff**: 无（即已对齐，sync 步为 no-op）
- **结论**: 格式同步无副作用

## 4. `cargo machete --with-metadata`

### 4.1 完整输出（去 DEBUG 噪音后）

```
cargo-machete found the following unused dependencies in this directory:
synthia-session -- ./crates/synthia-session/Cargo.toml:
	async-trait
	synthia-core
	synthia-protocol
synthia-job -- ./crates/synthia-job/Cargo.toml:
	tokio-test
synthia-tool -- ./crates/synthia-tool/Cargo.toml:
	dashmap
	libc
	thiserror
	tokio-util
synthia-agent -- ./crates/synthia-agent/Cargo.toml:
	nix
	opentelemetry-otlp
	opentelemetry-stdout
	opentelemetry_sdk
	reqwest
	tokio-stream
	tracing-opentelemetry
synthia-a2a -- ./crates/synthia-a2a/Cargo.toml:
	a2a-client-lf
	a2a-server-lf
	async-trait
	dashmap
	serde
	synthia-tool
	thiserror
	tokio
	tokio-util
	url
	uuid
synthia-core -- ./crates/synthia-core/Cargo.toml:
	tokio-util
synthia-protocol -- ./crates/synthia-protocol/Cargo.toml:
	chrono
	pretty_assertions
synthia-server -- ./crates/synthia-server/Cargo.toml:
	a2a-pb
	base64
	serial_test
	synthia-protocol
	thiserror
	tokio-tungstenite
synthia-telemetry -- ./crates/synthia-telemetry/Cargo.toml:
	thiserror
synthia-provider -- ./crates/synthia-provider/Cargo.toml:
	anyhow
	async-stream
	tempfile
	uuid
synthia-command -- ./crates/synthia-command/Cargo.toml:
	thiserror
test-support -- ./test-support/Cargo.toml:
	anyhow
	async-stream
	chrono
	futures
	serde
	synthia-skill
```

### 4.2 误报警示

`cargo-machete` 已知在以下场景会**误报**未用依赖：

1. **`#[cfg(test)]` + dev-dependencies 实际被 integration tests 引用**
2. **通过 `pub use` 重导出**（machete 不追踪 re-export 链）
3. **`build.rs` / `examples/` / `tests/` 引用**（在 `[[example]]` / `[[test]]` 段）
4. **feature-flagged 依赖**（仅在 `feature = "xxx"` 启用时被引用）
5. **二进制 crate 的依赖**（machete 偏重 library crate）
6. **`a2a-pb` / `a2a-client-lf` / `a2a-server-lf`** 等 vendor-a2a-pb 子 crate 是 `[patch.crates-io]` 注入的，machete 不识别 patch 路径

### 4.3 候选清单分类（按需后续核查）

| 类别 | 候选 | 备注 |
|---|---|---|
| **高置信度未用** | `crates/synthia-agent/Cargo.toml: reqwest` | 仓库不直接调 HTTP 客户端（API 调用都走 `synthia-provider`），但需 grep 确认 |
| **高置信度未用** | `crates/synthia-agent/Cargo.toml: nix` | 需 grep `use nix::` 确认无引用 |
| **可能 feature-flag** | `crates/synthia-agent/Cargo.toml: opentelemetry-otlp / opentelemetry-stdout / opentelemetry_sdk / tracing-opentelemetry` | 可能只在 OTel feature 启用时引用，machete 不追踪 feature gate |
| **可能 dev-only** | `crates/synthia-server/Cargo.toml: serial_test` | 典型 dev-dependency 误报 |
| **可能 build.rs** | `crates/synthia-agent/Cargo.toml: opentelemetry-otlp` | 可能在 build.rs 引用 |
| **patch 注入** | `crates/synthia-server/Cargo.toml: a2a-pb` | 由 `[patch.crates-io]` 注入 |
| **test-support 内** | `test-support/Cargo.toml` 多个 | 集成测试 fixture 用，machete 不追踪 cross-crate test |
| **待逐项核查** | 其余 ~30 项 | 需 `grep -rn "use <dep>" crates/<crate>/` + `Cargo.toml` 双向确认 |

### 4.4 本提案决策

**仅记录清单，不修改**。理由：
- 改动面爆炸：30+ 候选需逐项 grep 确认
- 误报风险高：6 大已知误报场景
- 不在本提案 scope：本提案核心是"删除残留 + 新扫描副产物"
- 建议后续变更：开 `2026-08-XX-machete-followup` 提案，按 crate 分批核查 + 修正

## 5. `cargo udeps --all-targets`

- **状态**: **未执行**
- **错误**: `error: no such command: 'udeps'`
- **原因**: 本机 cargo 未安装 `cargo-udeps`（需 `cargo install cargo-udeps --locked`）；本提案避免引入新安装步骤
- **替代**: machete 已覆盖大部分"未用依赖"场景（精度不同：machete 偏静态文本扫描，udeps 偏编译图实测）
- **建议**: 如需 udeps 精确结果，开新变更安装 + 跑
- **关联**: 待 4.3 候选清单逐项确认后，udeps 可作为最后验证步骤

## 6. 综合健康度

| 维度 | 状态 | 备注 |
|---|---|---|
| clippy | 0 warning | 干净 |
| fmt | 0 diff | 干净 |
| test（base） | 0 baseline 失败 | 待 tasks.md Layer 4 分批跑确认 |
| machete | 30+ 疑似未用 | 需后续变更核查 |
| udeps | 未执行 | 需后续安装 |

## 7. 与前一轮 archive 提案的关系

- 本次扫描是**前一轮 `2026-08-01-deep-cleanup-mvp-aligned-workspace` 之后 ~60 个 commit 的健康度快照**
- 0 warning + 0 diff 表示该 archive 变更没有引入回归
- machete 报告的 30+ 候选大多是**长期存在**的（多个候选在前一轮 archive 之前就存在），不属于"自 archive 后新冒出"的死代码
- 本提案不强制处理，留给后续变更决策
