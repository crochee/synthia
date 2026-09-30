# Security Policy

Synthia 项目按 CNCF 社区通行做法处理安全问题：私下报告 + 修复发布 +
公开披露协调。**请勿在公开 issue / PR / 讨论区披露未修复的安全漏洞。**

## 支持的版本

| 版本  | 状态                           |
| :---- | :----------------------------- |
| 最新 release (`crates/synthia` facade 的 `Cargo.toml` 版本) | **支持** |
| 之前版本 | 不支持 —— 升级到最新 release |

`Cargo.toml` 的 `[workspace.package] version` 是单一真源；发布 tag 与
该字段一致，由 `.github/workflows/release.yml` 在推送 `vX.Y.Z` tag 时
自动出多平台产物（见 [RELEASE.md](RELEASE.md)）。

## 报告漏洞

通过 GitHub 的
[私有漏洞报告](https://github.com/crochee/synthia/security/advisories/new)
提交；若不可用，联系维护者（见 [MAINTAINERS.md](MAINTAINERS.md)）。

报告里请包含：

- 受影响版本 / commit（`Cargo.lock` 中的 `synthia` 系列 entry 即可）；
- 复现步骤或 PoC（最小化的 `cargo` 命令 / 模型请求片段 / 配置文件均可）；
- 影响面评估（执行能力？数据外泄？拒绝服务？）；
- 你倾向的披露时间表。

**响应承诺**：72 小时内确认收到；修复与披露节奏与报告者协商（通常
90 天内）。修复发布后在 release notes 里致谢（除非要求匿名）。

## 威胁模型

Synthia 是一个**运行时无关的 Rust AI Agent 框架**，核心通过
[`synthia_core::CancelToken`](crates/synthia-core/src/cancel.rs) 与
[`synthia_core::spawn::Spawner`](crates/synthia-core/src/spawn.rs)
抽象把工具 / 模型 / 调度逻辑与宿主运行时隔开。范围内外的判定与
`AGENTS.md` §3.7 的"可作为 lib"原则保持一致。

### 范围内

- 公开 API 中**可观察**的内存安全 / 数据完整性缺陷（崩溃、use-after-free
  、越权写入、任意代码执行路径）；
- `synthia-tool-shell` 的 `ShellCommands.json` 等配置文件解析路径的
  panic / 无界资源消耗（`synthia-tool-shell::sandbox` 是 OS 执行策略
  的单一收口，`AGENTS.md` §3.7）；
- 公开 crate 暴露的 HTTP / IPC 表面（如 `synthia-server` 的 axum 路由、
  `synthia-mcp` 的 MCP transport）的拒绝服务 / 注入；
- 凭据 / 密钥在源代码、release artifact、文档、CI 日志里的泄漏。

### 范围外（设计使然，文档已声明）

- 宿主应用把 `Arc<ModelProvider>` 装到 `ReActAgent::new(provider, …)`
  之后，由该 provider 自身的能力边界决定 —— Synthia 不替宿主裁决
  "是否应该让模型执行 X"，`AGENTS.md` §3.7 的"七大组件每一件都是
  可换的 trait + 默认实现"是这一点的设计依据；
- 上游依赖（`reqwest` / `axum` / `tokio` / 模型 SDK）的漏洞 —— 请同时
  向上游报告；
- 模型输出内容的"安全性"（prompt injection / 模型行为对齐）—— Synthia
  把这层视作宿主责任，提供 `synthia-steering` 的 `Guard` / `Hint` /
  `Hook` 机制让宿主接入，但不在框架内做对齐；
- 配置文件（如 `config.yaml`）由宿主应用解释 —— Synthia 不解析它。

## 凭据与密钥

- `.env` / `config.yaml` 不得入库（已在 `.gitignore` 顶端声明）；
- CI 的所有密钥经 GitHub Actions Secrets 注入，不在 workflow 文件里
  落明文（`.github/workflows/` 内的 `secrets.<…>` 引用为唯一路径）；
- 测试夹具里的"假密钥"必须前缀 `test-` 或后缀 `-dummy`，方便在
  凭据扫描里被过滤（`make check-secret-patterns`，待 R 后续开启）。

## 致谢

修复发布后致谢名单：

- 暂无。

报告漏洞请按上方流程，**不要**公开贴出 PoC。

## CVE 编号与公开披露协调

Synthia 项目按 [CNCF Security](https://github.com/cncf/tag-security) +
[GitHub Advisory Database](https://github.com/advisories) 的流程处理：

1. **CVE / GHSA 编号**：通过 GitHub 私有漏洞报告提交后，维护者负责与
   GitHub Security Advisories 工作流对接申请 **GHSA** 编号；如需
   **CVE** 编号则经 [MITRE](https://cve.mitre.org/) 或其授权的
   CVE Numbering Authority（CNA）授权。编号由维护者持有，**不**对
   报告者可见直至公开披露。
2. **修复发布**：GHSA / CVE 编号在补丁随 release tag 发布后同步
   公开；release notes 引述 GHSA 链接（`https://github.com/crochee/synthia/security/advisories/GHSA-…`）。
3. **公开披露时间线**：与报告者协商，默认 **90 天**（与 Google
   Project Zero 等业界共识一致）。如需推迟或加速，提供理由。
4. **致谢名单**：披露时显式列出报告者（除非要求匿名），含修复版本
   与影响范围。详见上节"致谢名单"。

## 政策与流程

- **私报渠道**：GitHub 私有漏洞报告 + 维护者直邮（见 [MAINTAINERS.md](MAINTAINERS.md)）。
- **不公开 PoC**：本文件 §"报告漏洞"已声明。
- **加密**：目前不接受 PGP 加密报告；如需加密，与维护者协调。
- **范围更新**：本文件的"威胁模型" / "范围外"段随每次 release 复审一次。

## 安全自动化

| 检查 | 工具 | 频率 | 入口 |
| :--- | :--- | :--- | :--- |
| Rust 公告数据库 | `cargo audit` (rustsec) | 每周一 03:00 UTC + 每次 PR | `.github/workflows/security-audit.yml` |
| 许可证 / 来源 / ban | `cargo deny` | 每次 PR + 每周一 | 同上 + `make check-deny` |
| 密钥 / 凭据 | (gitleaks 已配置待启用) | 每次 PR | `.github/workflows/security-audit.yml` |
| 依赖自动升级 | Dependabot | 每周一 09:00 UTC | `.github/dependabot.yml` |
| 制品来源证明 | sigstore + SLSA Build L3 | 每次 release | `.github/workflows/release.yml` |

报告漏洞请按上方流程，**不要**公开贴出 PoC。