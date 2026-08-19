# env

真实llm api配置在.env中

## OTel (可选)

启用 `otel` cargo feature 后，通过环境变量配置 OpenTelemetry tracing：

- `SYNTHIA_OTLP_ENDPOINT` — OTLP collector 地址，scheme 自动选择 gRPC/HTTP
  （`grpc://` / `https://` / 无 scheme → gRPC；`http://` → HTTP，4317 端口例外走 gRPC）
  未设置时退化为 console tracing。
- `SYNTHIA_OTEL_SAMPLER` — 采样器覆盖（`always_on` / `always_off` / `trace_id_ratio:0.1`），
  默认 `ParentBased(AlwaysOn)`。设置后包裹 `ParentBased` 以兼容父 trace 采样决策。

详见 [crates/synthia-telemetry/README.md](crates/synthia-telemetry/README.md)。

# 代码同步规范

- 不主动push代码到远程仓库
- find时不到万不得已，不使用find命令且是根目录下的文件
- 搜索时，优先级应该是本地工作空间>HOME目录>其他目录
- 统一使用 `rg`（ripgrep）替代 `grep`
- 统一使用 fd (fd-find) 替代 find

