# Release Process

semver、可复现产物、零手工步骤 —— 产物全部由 CI 构建，人只做决定。
tag 触发 [`.github/workflows/release.yml`](.github/workflows/release.yml)，
构建并推送两个跨平台容器镜像（`docker.io/crochee/synthia-server` +
`docker.io/crochee/synthia-web`，linux/amd64 + linux/arm64），OCI
`org.opencontainers.image.{revision,version,created}` 自证身份
（详见 §"供应链保障"）。

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
4. **CI 出产物**：tag 触发
   [`.github/workflows/release.yml`](.github/workflows/release.yml)，
   产出两个跨平台容器镜像：
   - `docker.io/crochee/synthia-server`（linux/amd64 + linux/arm64）；
   - `docker.io/crochee/synthia-web`（linux/amd64 + linux/arm64）。

   每个镜像打 tag 两件套：`<tag>` 直用 + `<tag>-<short-sha>` 追溯。
   （workflow 仅在 `v*.*.*` 形式的 tag push 触发，所以
   `<branch>-<short-sha>` 暂未启用。）OCI
   `org.opencontainers.image.{revision,version,created}` 由 buildx
   现场 inspect 与 tag 关联 SHA 对齐校验。镜像直接 push 到
   Docker Hub，不创建 GitHub Draft release — release notes 由
   GitHub tag 创建时自动生成的 `auto-generated notes` 接手，或
   maintainer 在 UI 手填。
5. **发布**：审阅 GitHub UI 上的 tag notes → 公开 → 发布。

## 复现 / 校验

```sh
# 1. 拉镜像
docker pull docker.io/crochee/synthia-server:vX.Y.Z
docker pull docker.io/crochee/synthia-web:vX.Y.Z

# 2. 容器镜像身份（OCI labels 与 tag 关联 SHA 对齐）
docker buildx imagetools inspect docker.io/crochee/synthia-server:vX.Y.Z \
  --format '{{json .Image.Labels}}' | jq -r '."org.opencontainers.image.revision"'

# 3. 容器内 binary 身份（与 OCI labels 同源, 都是 SYNTHIA_GIT_SHA)
docker run --rm docker.io/crochee/synthia-server:vX.Y.Z --version
```

镜像 tag 两件套（`<tag>` / `<tag>-<short-sha>`）由 release job
的 `images` step 上传到 Docker Hub；`<tag>-<short-sha>` 给无法滚动
但要精确回滚的场景。
镜像目前**未**走 cosign keyless 签名（keyless signing for OCI images 需要
额外的 `cosign sign` 步骤与 registry-side 配置，参见后续路线）。

## 后续路线

- 镜像签名：接 `cosign sign` 给每个 multi-arch manifest list 出 `.sig`，
  与现有 binary 签名一致；需要 `id-token: write` + registry 信任链。

如果 `sha256sum` 或 `<commit>` 不匹配，立即在 issue 上报。

## 供应链保障

每个 release 都伴随以下保障（OCI image + metadata）：

| 资产 | 工具 / 流程 | 触发 |
|---|---|---|
| 镜像 manifest | `docker buildx build --push` (linux/amd64 + linux/arm64) | `release.yml` `Build and push synthia-{server,web}` step |
| OCI image labels | `Dockerfile.{server,web}` 烤入 `org.opencontainers.image.{revision,version,created}` | build arg `SYNTHIA_GIT_SHA` / `SYNTHIA_VERSION` / `SYNTHIA_BUILD_TIME` |
| Identity 校验 | `docker buildx imagetools inspect --raw` + `jq` | `release.yml` `Verify image labels` step |

第三方校验路径：

| 校验方 | 怎么读 | 工具 |
|---|---|---|
| 终端用户 | `docker pull` + `--rm <image> --version` | `docker` |
| 平台 / K8s | `kubectl describe pod ...` 看 `Image:` | `kubectl` |
| 审计 / 监管 | `docker buildx imagetools inspect` 读 OCI labels | `docker` + `jq` |

OCI labels 覆盖 [NTIA Minimum Elements](https://www.ntia.gov/sbom) 的
版本 + 来源字段；SBOM 后续由 `cosign attach sbom` 接入（参见后续路线）。

## 后续路线

- 镜像签名：接 `cosign sign` 给每个 multi-arch manifest list 出 `.sig`，
  配套 `actions/attest-build-provenance` 出 in-toto provenance，组合成
  SLSA Build L3 标识（与上游二进制 release 的 `id-token: write` 同条件）。
- 镜像 SBOM：接 `cosign attach sbom` 把 `cargo-cyclonedx` 的 SBOM 钉
  到 manifest 上，给企业审计 / EU CRA 用。
- 包仓库：crates.io 由 `cargo publish` 触发（`crates/synthia` facade
  + `synthia-core` 起步），R 后续开启 `release.yml` 的 publish job。