# Maintainers

本文件按 CNCF 社区的 `MAINTAINERS.md` 模板列出有合 PR 权限的维护者，
并标明各自的关注面。新维护者的添加 / 移除由现有维护者一致决定。

## 当前维护者

| 姓名 | GitHub | 关注面 |
| :--- | :--- | :--- |
| crochee | [@crochee](https://github.com/crochee) | 全面（facade / harness / server / CI / docs） |

## 关注面

按目录/crate 划分的责任边界（与 [`.github/CODEOWNERS`](.github/CODEOWNERS) 对齐）：

| 目录 / crate | 关注面 |
| :--- | :--- |
| `crates/synthia-core/` / `crates/synthia-macros/` | 核心 trait（`CancelToken` / `Spawner` / `Clock` / `IdGen` / `Registry`）；proc-macro `#[derive(Tool)]` |
| `crates/synthia-provider/` | 模型适配（Anthropic / OpenAI compatible），wire type 与 streaming |
| `crates/synthia-tool/` / 8 个 tool plugin crate | 工具范式 + 离线工具集 |
| `crates/synthia-harness/` | ReAct 循环 + `Agent` / `ToolInterceptor` / strategy seam |
| `crates/synthia-server/` | axum HTTP / SSE / `config.yaml` 加载 + 启动 |
| `synthia-web/` | React / TS UI；MCP 浏览器面 |
| `.github/workflows/` / `Makefile` / `scripts/` | CI / 自动化 / 棘轮 |
| `docs/` / 顶层 `*.md` | 文档体系（`MINIMAL` / `SEAMS` / `AGENTS` / `CHANGELOG`） |
| `crates/synthia-search/` / `crates/synthia-tool-search/` | 通用搜索引擎 + 跨域 search 工具 |
| `crates/synthia-scheduler/` / `crates/synthia-tool-scheduler/` | 调度运行时 + `schedule` 工具 |
| `crates/synthia-workflow/` | 多智能体 workflow runtime |

## 权限授予

新维护者提 PR 修改 [`CONTRIBUTING.md`](CONTRIBUTING.md) +
[`.github/CODEOWNERS`](.github/CODEOWNERS) 与本文件，并 @ 现有维护者
确认；至少一名现有维护者 approve。

## 退出流程

维护者主动退出时，提 PR 把自己的条目从本文件 / `CODEOWNERS` /
`config.yaml`（如适用）中移除。

## 不活跃维护者

连续 6 个月无 commit / review / issue 响应的维护者，由其他维护者在
内部讨论后从名单移除。