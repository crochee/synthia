# Roadmap — Synthia

> 公开路线图。短期（≤ 1 个 milestone）/ 中期（1–3 个 milestone）/ 长期
> （vision）三档；每个条目带可验证证据。改动通过 `docs/superpowers/specs/`
> 下的 spec 文件跟踪；本文件是 `CHANGELOG.md` 的"前瞻"对应物。

## 状态符号

- ✅ 已交付
- 🚧 进行中
- 📋 计划中
- 🔮 vision（无明确时间）

## 短期（下一里程碑）

### 🚧 CI/CD 完整化（R161）
- ✅ `.github/workflows/ci.yml`：fmt / clippy / test / examples / MSRV
- ✅ `.github/workflows/release.yml`：tag 触发三平台产物
- ✅ `.github/workflows/security-audit.yml`：cargo-audit + cargo-deny
- ✅ `.github/workflows/docs.yml`：rustdoc → gh-pages
- ✅ `.github/workflows/labeler.yml`：PR 自动标签
- ✅ `deny.toml`：许可证 / 来源 / 公告 / ban 完整策略
- ✅ `SECURITY.md` / `CONTRIBUTING.md` / `RELEASE.md` / `MAINTAINERS.md`
  / `.github/CODEOWNERS` / `.github/SUPPORT.md` / `dependabot.yml`
- 📋 把 `make ci` 入口接到新 workflow（已在 Makefile 加 `check-deny`，
  CI 链路对齐）

### 🚧 crates.io 发布（R162）
- 📋 `cargo publish` workflow：`crates/synthia` facade + `synthia-core`
  起步；后续按依赖方向批量推；
- 📋 文档站点 `docs.rs/synthia` 自动接入（已在 `.github/workflows/docs.yml`
  配）；
- 📋 给每个 crate 加 `categories = ["api-bindings", ...]` 与
  `keywords = [...]` 改善 crates.io 搜索结果。

### 🚧 仓库规范完成（R163）
- ✅ `AGENTS.md` 全面（v2.3+）；
- ✅ `MINIMAL.md` / `SEAMS.md` / `README.md` / `CHANGELOG.md`；
- ✅ 棘轮脚本（`check-mvp-deps` / `check-no-runtime` /
  `check-public-api-runtime` / `check-pub-surface` /
  `check-harness-shape` / `check-clock` / `check-test-layout`）；
- ✅ Lint 网（`clippy.toml` + `scripts/harness-shape/clippy.toml` + 形状预算）。

## 中期（1–3 个里程碑）

### 📋 1.0 API 稳定（R170）
- 所有 spec re-export: trait / type signature 在 `synthia-core` 锁定；
- 公开 facade feature 集合冻结；
- 升级路径文档：`docs/MIGRATION-0.x-to-1.0.md`。

### 📋 SLSA Build L2（R172）
- 给仓库配置 OIDC-enabled token；
- `.github/workflows/release.yml` 加 `actions/attest-build-provenance@v2`；
- 来源证明：tag + sha256 + OIDC in-toto attestation = 三件套。

### 📋 跨平台扩展（R175）
- Linux aarch64 产物（GitHub-hosted runner + cargo cross）；
- Linux RISC-V（待需求）；
- WASM 编译目标（`synthia-harness` 纯异步跑在浏览器中）—— R 后续开 spec。

### 📋 评测 / 回归护栏（R178）
- `synthia-eval` 的 golden-transcript 套件跑 CI；
- `bench-check` 收窄 hot-path 比例（仓内已有 `make bench-check`，本
  milestone 是把噪声点收齐）。

## 长期（vision）

### 🔮 多模态一等公民
- `synthia-attachment` 升级为"模型上下文图片 / 音频 / 文件 bytes
  一等公民"，每件工具原生可消费 `AttachmentRef`（已部分实现）。

### 🔮 商业化配套
- 双轨发布（crate + raw binary）已就位；
- SLA / 私有 fork / 培训 / 咨询通道：通过 GitHub Discussions 与
  仓库所有者直接对接；
- 与上游 LLM 提供商签合作（Anthropic / OpenAI）以扩大可商业化场景，
  R 后续在 `docs/COMMERCIAL.md` 跟踪。

### 🔮 行业标准对齐
- CNCF Sandbox / TAG App Delivery 的 AI agent 实践（参考 [pi-mono]、
  [DeepSeek DSH]）；
- OpenTelemetry GenAI Semantic Conventions（`synthia-telemetry/otlp`
  跟进）；
- MCP 规范（`synthia-mcp` 跟进）。

## 不做（out of scope）

我们**不**计划做：

- 一个"全功能超级 app"框架（不是 LangChain / pi-mono）—— 我们是 lib
  + 一份最小 server；
- 模型微调 / RLHF（交给上游）；
- 一个统一的"agent store / marketplace"（让 plugin 自由 + 各自分发，
  不集中托管）；
- 强类型 schema 强约束（用 `serde` + JSON Schema 衍生，但不强加
  代码生成）。

## 一句话目标

> 成为 Rust AI agent 生态的"标准乐高套件"—— 一行 `cargo add synthia`
> 起步，从 0 到能上生产的 AI agent 都由同一套 trait 拼出来。

---

[pi-mono]: https://github.com/badlogic/pi-mono
[DeepSeek DSH]: https://github.com/deepseek-ai/dsh

> 本文件与 [`CHANGELOG.md`](../CHANGELOG.md) `[Unreleased]` 段同步更新；
> 每条交付都应在 CHANGELOG 留一行证据。