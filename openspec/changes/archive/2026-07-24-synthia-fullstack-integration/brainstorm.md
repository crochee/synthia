<!--
Raw capture of brainstorming session.

本檔原樣捕捉 brainstorming 的產出，記錄決策鏈與設計取捨。
design.md 從本檔萃取並重新整理為結構化設計文件。
-->

## 背景

Synthia 项目前后端严重脱节：
- Server 已重构为 A2A 协议驱动，删除了 `/api/sessions`、`/ws` 等旧端点
- Web 仍在调用已不存在的端点（`/api/sessions`、WebSocket `ws://localhost:3000`）
- 无 Makefile、无 Dockerfile、无 E2E 测试、无设计系统
- 用户希望整体一次性设计，打通前后端联调

## 决策链

### Q1: 范围拆解 vs 整体设计？
- 选项：按子系统拆解（4 个独立 spec） vs 整体一次性设计
- **决议：整体一次性设计**
- 理由：用户明确要求整体设计，前后端需要统一规划

### Q2: 前后端通信协议？
- 选项 A: Web 直接对接 A2A 协议
- 选项 B: Web 通过 A2A 封装层
- **决议：使用官方 `@a2a-js/sdk` (npm install @a2a-js/sdk)**
- 理由：官方 SDK v1.0 稳定版，支持 JSON-RPC、HTTP+JSON/REST、gRPC 三种传输

### Q3: 设计风格？
- 选项 A: 霓虹终端 (Neon Terminal) — 纯黑底 + 霓虹绿/青 + 等宽字体
- 选项 B: 深空科幻 (Dark Sci-Fi) — GitHub Dark 色调 + 蓝色强调
- 选项 C: 玻璃未来 (Glass Future) — 深紫渐变 + 毛玻璃效果
- 选项 D: 当前风格精修 (Current Refine) — 保持现有暗蓝+品红
- **决议：A. 霓虹终端 (Neon Terminal)**
- 设计 token: `--bg-primary: #0a0a1a`, `--accent-green: #00ff88`, `--accent-cyan: #00ddff`, `--font-mono: 'JetBrains Mono'`

### Q4: Web 功能范围？
- P0: 聊天界面、会话管理、连接状态
- P1: 工具面板、技能面板、设置页面
- P2: 任务面板、记忆搜索、作业调度、MCP 管理
- **决议：全部 P0+P1+P2**

### Q5: 部署模式？
- 选项 A: 单体部署（前端嵌入后端二进制）
- 选项 B: 前后端分离部署
- 选项 C: 混合模式
- **决议：B. 分离部署**（用户最终选择）
- 前端通过 Nginx 反向代理提供服务，后端仅服务 API
- Nginx 配置需处理 SSE streaming（proxy_buffering off）

### Q6: E2E 测试范围？
- 层级 1: UI 交互测试
- 层级 2: 前后端联调测试
- 层级 3: Agent 功能逻辑测试
- **决议：全部覆盖**

### Q7: 开发方式？
- 选项 A: 渐进式迁移
- 选项 B: 并行开发
- 选项 C: 分层迭代
- **决议：B. 并行开发**
- 前端轨道、后端轨道、集成轨道并行进行

## 设计取捨

### A2A SDK 连接状态语义
- A2A SDK 使用 HTTP/SSE 而非 WebSocket
- `isConnected` 改为 `isServerAvailable`，通过 `/health` 端点定期检测
- 每 30 秒轮询一次健康检查

### Vite 代理配置
- 需同时代理 `/api` 和 `/a2a` 到后端
- 开发模式：前端 :5173，后端 :8080

### Docker 多阶段构建
- Rust 镜像使用 `rust:1.95-alpine`
- 前端镜像使用 `nginx:alpine` 提供静态文件
- 开发环境使用 `Dockerfile.server.dev` 和 `Dockerfile.web.dev`

### 后端 CORS 配置
- 新增 `CorsConfig` 结构体到 `ServerConfig`
- 默认允许 `http://localhost:5173`（Vite 开发服务器）
- 使用 `tower-http` 的 `CorsLayer`

### 移除静态文件嵌入
- 分离部署模式下后端不再嵌入前端产物
- 移除 `embedded-ui` feature 和 `rust-embed` 依赖

## 实施阶段

1. **Phase 1: Foundation** — CORS 配置 + A2A SDK 最小集成 + Makefile + Dockerfiles
2. **Phase 2: Design + Core** — Neon Terminal 设计系统 + React Router + 聊天界面重构
3. **Phase 3: Management** — 工具/技能/设置/任务/记忆/作业/MCP 页面
4. **Phase 4: Testing** — Playwright 集成 + 三层 E2E 测试
5. **Phase 5: Deployment** — Docker Compose 生产配置 + Nginx 反向代理 + 文档
