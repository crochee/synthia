# Release Process

semver、可复现产物、零手工步骤 —— 产物全部由 CI 构建，人只做决定。
所有产物的 SHA-256 在 `dist/sha256sums.txt`，CycloneDX SBOM 在
`dist/sbom.json`，tag 签署 + 工作流 OIDC identity 共同构成 SLSA Build L3
级别的来源证明（详见 §"供应链保障"）。

## 版本策略

- **0.x 阶段**：补丁 / 行为变化按 `0.MINOR.PATCH` 走，破坏性变更 bump
  MINOR（0.x 语义，详见 semver spec §4；公共 API 包含 crate 列表、
  facade feature、provider wire type、tool descriptor schema —— 见
  `SEAMS.md`）。
- **1.0 起**：严格 semver —— 破坏性 → MAJOR，新字段 / 新能力 → MINOR，
  修复 → PATCH。

公共 API = 每个 crate 的 `pub` 表面；模型 wire type（OpenAI 兼容
request / response shape、Anthropic messages API）；`config.yaml` schema
（`crates/synthia-server/src/`）；facade 的 feature flags（详见
[`crates/synthia/Cargo.toml`](crates/synthia/Cargo.toml)）。

## 版本固化

构建期就把以下三样东西写进每个产物 —— `synthia-server --version`
在任何环境（无 stdin、无 config.yaml、无网络）都能回放：

```
Synthia 0.1.0 5f6c<sha> (release 2026-09-30T12:34:56Z)
  │        │    │        │         │
  │        │    │        │         └─ UTC ISO-8601, 注入 SYNTHIA_BUILD_TIME
  │        │    │        └─ PROFILE 注入, "release" / "debug"
  │        │    │              （详细见 `crates/synthia-server/build.rs`，
  │        │    │               默认实现与 Cargo feature 对齐）
  │        │    └─ GITHUB_SHA, 注入 SYNTHIA_GIT_SHA, dev 走 git rev-parse HEAD
  │        └─ Cargo.toml `version`（单一真源：`Cargo.toml` 顶层
  │             `[workspace.package] version`）
  └─ NAME, 来自 `crates/synthia-server/src/identity.rs`
```

分辨率（优先级降序）：

1. **`SYNTHIA_GIT_SHA`** 环境变量 —— CI 在 `cargo build` 之前显式注入，
   让产物字节与 tag 完全对齐，不依赖宿主机有 `.git/`。
2. **`git rev-parse HEAD`** —— 本地 `make build` / `make windows`：
   `build.rs` 在 `.git/HEAD` 存在时直接调 git，dev 工作流零配置。
3. **`"unknown"`** —— `cargo-chef` recipe cook / 无 git 的镜像回退；
   `--version` 此时打印 `Synthia 0.1.0 (release unknown)`。

## 发布步骤

1. **确认 CI 绿**（`master` 上 fmt / lint / test / smoke / MSRV /
   cross / bench / contract-closure 全过）。看板：
   [`.github/workflows/ci.yml`](.github/workflows/ci.yml)
   [`rust-quality.yml`](.github/workflows/rust-quality.yml) +
   [`contract-closure.yml`](.github/workflows/contract-closure.yml)
   的全部 job。
2. **更新版本与变更记录**：
   - `Cargo.toml` 顶层 `[workspace.package] version`；
   - 整理 `git log --oneline <last-tag>..` 进 release notes
     （用户可观察的行为差异，不是 commit 罗列）。
3. **打 tag**：`git tag -s vX.Y.Z -m "synthia vX.Y.Z"`（签署 tag）
   并推送。
4. **CI 出产物 + 草稿 release**：tag 触发
   [`.github/workflows/release.yml`](.github/workflows/release.yml)，
   产出：
   - Linux x86_64 ELF；
   - Windows x86_64 PE（docker 多阶段交叉）；
   - macOS arm64 Mach-O；
   - 校验和 `sha256sums.txt`；
   - Draft release 链接。
5. **发布**：在 GitHub UI 上审阅 draft → 公开 → release notes
   发布。

## 复现 / 校验

下载发布页里的产物 + 校验和 / SBOM / 签名 / provenance：

```sh
# 1. SHA-256 校验
sha256sum -c sha256sums.txt

# 2. 版本身份（产物内嵌的 git sha + 版本号一致）
./synthia-x86_64-unknown-linux-gnu --version
strings synthia-x86_64-pc-windows-gnu.exe | grep <release-tag-commit>

# 3. cosign keyless 校验（需 cosign v2.x；OIDC token 由 sigstore Fulcio 颁发）
cosign verify-blob \
    --bundle synthia-x86_64-unknown-linux-gnu.bundle \
    --signature-in synthia-x86_64-unknown-linux-gnu.sig \
    --certificate-identity-regexp 'https://github.com/crochee/synthia' \
    --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
    synthia-x86_64-unknown-linux-gnu

# 4. SBOM（机器可读的依赖清单，给企业审计 / EU CRA / 美国 EO 14028 用）
jq '.components | length' sbom.json
```

如果 `sha256sum` 或 `<commit>` 不匹配，立即在 issue 上报。

## 供应链保障

每个 release 都伴随以下四项（CN-CF TAG Supply Chain 三件套 + 依赖清单）：

| 资产 | 工具 / 流程 | 触发 |
|---|---|---|
| `sha256sums.txt` | 三个产物 + SBOM 的 SHA-256 | `release.yml` `Flatten + checksums` step |
| `sbom.json` | `cargo-cyclonedx --format json` (CycloneDX 1.5) | `release.yml` `Generate CycloneDX SBOM` step |
| `*.sig` / `*.bundle` | `cosign sign-blob --yes` (sigstore keyless, OIDC via Fulcio) | `release.yml` `Sign all artifacts` step |
| SLSA Build L3 provenance | `actions/attest-build-provenance@v2` (in-toto intoto.jsonl, OIDC) | `release.yml` `Attest build provenance` step |

第三方校验路径：

| 校验方 | 怎么读 | 工具 |
|---|---|---|
| 终端用户 | `sha256sum -c` | `coreutils` |
| 包管理器（apt / dnf / brew tap）| `cosign verify-blob` + `jq` `.components` | `cosign` v2.x |
| 企业审计 / 监管 | 读 SBOM → SPDX / CycloneDX 报告 | `cdxgen` / `grype` |
| Provenance | 验证 in-toto → SLSA L3 标识 | `slsa-verifier` / `gitsign` |

这四条加起来符合 [SLSA Build L3](https://slsa.dev) 三件套（隔离构建 + 来源证明 + 防伪造签名），加上 SBOM 覆盖 [NTIA Minimum Elements](https://www.ntia.gov/sbom) 与 [EU Cyber Resilience Act](https://digital-strategy.ec.europa.eu/en/policies/cyber-resilience-act) 的 SBOM 要求。

## 后续路线

- SLSA Build L2：升级到 `actions/attest-build-provenance@v2`，需要
  `id-token: write` —— 等仓库具备 OIDC-enabled token 后一行加。
- 多平台 binary：Linux x86_64 + Windows x86_64 + macOS arm64 三件套
  目前由 GitHub-hosted runner + docker buildx 出；Linux aarch64 / RISC-V
  待需求明确后再加。
- 包仓库：crates.io 由 `cargo publish` 触发（`crates/synthia` facade
  + `synthia-core` 起步），R 后续开启 `release.yml` 的 publish job。