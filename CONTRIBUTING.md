# Contributing to Synthia

感谢参与！本仓按 CNCF 社区的通行做法维护 —— 门槛低、流程可预期、一切
以 CI 为准。先读 [`README.md`](README.md) 了解项目是什么，再回来走
流程。要替换某个组件或写插件，读
[`SEAMS.md`](SEAMS.md) —— 那是 7 大组件的可替换点清单。

## 开发环境

需要：Rust 工具链（版本由 `rust-toolchain.toml` 钉死，rustup 自动装）；
可选：docker（仅交叉编译与契约测试时）、Node.js 20+（仅前端
`synthia-web/`）。不需要任何外部 API key —— 测试 / 模组 / 示例全部用
`replay.json` 类夹具或本地 stub。

```sh
git clone <repo> && cd synthia
make ci               # 改动后的全部护栏, 提交前必绿
make examples         # 全 example 与两个独立 consumer crate 的 proof line
```

`make ci` 是单点真相（详见 `AGENTS.md` §3.6）：fmt-check / clippy /
doc-check / test-guides / 七个依赖 / 布局 / 形状 / 时钟不变量目标。
它在 GitHub Actions `.github/workflows/ci.yml` 的 `gates` job 里
逐字再跑一次。

## 开发循环

```sh
make fmt              # rustfmt (nightly, 见 AGENTS.md §3.2)
make lint-rust        # clippy --all-targets --all-features --tests --all -D warnings
make test-crates      # 逐个 crate 运行完整测试 (见 AGENTS.md §3.3)
make examples         # example + 两个独立 consumer crate
make ci               # 上面四者的并集 + 其它不变量
```

改 `crates/<x>/src/` 后：`make ci`。改 `synthia-web/src/**` 后：
`make check-web`（`tsc --noEmit` + `eslint` + `prettier --check`），
然后用 Playwright MCP 验证真实渲染（`AGENTS.md` §4.2）。

## 架构一览

| 关注点 | 看哪里 |
|---|---|
| 我应该用哪个 crate | [`MINIMAL.md`](MINIMAL.md)（4 步 MVP guide） |
| 我能换掉哪个 seam | [`SEAMS.md`](SEAMS.md)（7 大组件的可替换点） |
| 一个新工具该怎么写 | [`crates/synthia-tool/`](crates/synthia-tool/) + 任一离线 tool crate（如 `synthia-tool-read`） |
| 多智能体委派 | [`crates/synthia-tool-task/`](crates/synthia-tool-task/)（`ToolInterceptor` seam 的第一个实现） |
| 调度 / cron / interval | [`crates/synthia-tool-scheduler/`](crates/synthia-tool-scheduler/) |
| 跨域搜索 | [`crates/synthia-search/`](crates/synthia-search/) + [`crates/synthia-tool-search/`](crates/synthia-tool-search/) |
| HTTP / WebSocket 表面 | [`crates/synthia-server/`](crates/synthia-server/) |

### 强制约定

- **依赖方向单向**：`synthia` → `synthia-*` → `synthia-core`，
  永远不回指；用 `cargo tree -p synthia-core` 验证无环。
- **可选的才是 opt-in**：默认 `synthia = "0.1"` 不会拉起 scheduler /
  workflow / search / skill / mcp / eval / attachment 七者之一；
  `make check-mvp-deps` 断言这一点。
- **不加依赖除非必要**；新增依赖走 `Cargo.toml` 的
  `[workspace.dependencies]` 单一收口，crate 内用 `dep = { workspace = true }`
  引用（见 `AGENTS.md` §3.1）。MSRV `1.95` 是硬上限。
- **公开面最小化**：只在 crate 外被实际使用的项才 `pub`；其余 helper
  必须 `pub(crate)` 或 `pub(super)`（`AGENTS.md` §3.7）。
- **运行时无关**：除 `synthia-server`（可选 feature）外，任何 crate
  都不应在公共 API 暴露 `tokio::*` / `async-std::*` / `smol::*`；
  `make check-public-api-runtime` 是这条线的门禁。
- **形状预算**：`synthia-harness` 生产代码不得有函数超过 100 行或 4 层
  嵌套，由 `make check-harness-shape` 断言（见
  [`scripts/harness-shape/clippy.toml`](scripts/harness-shape/clippy.toml)）。

## 提交规范

**Conventional Commits**（`feat:` / `fix:` / `docs:` / `refactor:` /
`test:` / `chore:` / `perf:` / `ci:`），一行说清"改了什么、为什么"。

**DCO 签核必备**：

```sh
git commit -s    # 生成 Signed-off-by: 你 <邮箱>
```

提交即表示同意以 MIT 许可贡献，且签署确认该贡献由你撰写 / 有权提交
（Developer Certificate of Origin，与 CNCF 项目一致）。

PR 标题形如 `feat(synthia-tool-read): add `binary` MIME classifier`；
GitHub 自动按 Conventional Commit 解析来生成 changelog 与版本号
（见 [RELEASE.md](RELEASE.md)）。

## 评审流程

1. **CI 全绿**：`make ci` 必须绿；任何红灯都不可合。Reviewer 看
   `.github/workflows/ci.yml` 的 `gates` / `tests` / `examples` 三个
   job 的状态贴即可。
2. **契约一致**：改了 facade / 公共 API 的 PR 必须更新
   `crates/*/src` 内受影响模块的 `pub use` 列表（`make check-pub-surface`
   棘轮）；改了 `crates/synthia-server` 路由或 `synthia-web` 调用的 PR
   必须保证 `contract-closure` 闭环（`.github/workflows/contract-closure.yml`）。
3. **不变量**：任何形变（lin / 形状 / 时钟 / 依赖 MVP）都在 CI 里有
   如果，直接红灯。
4. **DCO 通过**：`git commit -s` 必须生成 `Signed-off-by:` 行，CI 由
   DCO bot 自动校验（`dco` GitHub App 一次性安装即可，无需修改 PR
   模板）。缺少签名 → 红灯；reviewer 在 PR 注释里回复 `DCO: recheck`
   即可让 bot 重跑。
5. **naming**：见 [`MAINTAINERS.md`](MAINTAINERS.md)。

## 行为准则

本项目采用 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) — 在
[CNCF Code of Conduct](https://github.com/cncf/foundation/blob/main/code-of-conduct.md)
基础上的本项目语境化版本。维护者保留对违反行为的处置权（移除评论 /
临时禁言 / 永久封禁仓库访问）；报告渠道与响应承诺见 CoC 文档。

## 本地与 CI 不一致怎么办

```sh
make -n ci          # 打印 `make ci` 会跑的完整命令列表
cargo --version     # 确认是 rust-toolchain.toml 钉的版本
rustup show         # 确认默认 toolchain 是 stable
make doctor         # Makefile 自带的诊断目标（见 make doctor）
```

如果 `make doctor` 不存在，请在 issue 报该命令应该出现 —— 这是 R 后续
开启的目标之一。

## 反馈

- 改 PR / 修文档：直接提 PR；
- 用例 / 设计讨论：[GitHub Discussions](https://github.com/crochee/synthia/discussions)；
- 安全问题：[SECURITY.md](SECURITY.md)；
- 其它：[SUPPORT.md](.github/SUPPORT.md)。