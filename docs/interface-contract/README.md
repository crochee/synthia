# Synthia Interface Contract

本目录是 Synthia 前后端接口契约的"single source of truth"。

## 文件

| 文件 | 用途 |
|---|---|
| [contract.yaml](contract.yaml) | 机器可读的双侧并集契约表（YAML，单一来源，由 `contract-scan` 生成） |
| [contract.json](contract.json) | 同 `contract.yaml` 的 JSON 视图（CI / Playwright 解析用） |
| [contract.md](contract.md) | 人类可读衍生报告（由 `contract-report` 生成） |
| [SCHEMA.md](SCHEMA.md) | `contract.yaml` 字段定义与不变式 |
| [ARBITRATION.md](ARBITRATION.md) | 双侧冲突时如何决定修改方向 |

## 工具（`contract-closure/`）

| 命令 | 作用 |
|---|---|
| `npm run scan` | 扫描双侧源代码，写出 `contract.yaml` + `contract.json` |
| `npm run check` | 校验双侧对齐；非 0 退出码 = 有 dangling |
| `npm run report` | 从 `contract.json` 生成 `contract.md` |
| `npm run test` | 跑 vitest 单测 |

Makefile 等价目标（项目根目录）：
- `make contract-scan`
- `make contract-check`
- `make contract-report`
- `make test-contract-closure`
