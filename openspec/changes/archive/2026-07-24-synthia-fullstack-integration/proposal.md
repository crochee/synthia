## Why

Synthia 前后端严重脱节：后端已重构为 A2A 协议驱动，但前端仍调用已删除的旧端点。项目缺乏统一的设计系统、工程化工具链和 E2E 测试体系。本次变更旨在打通前后端联调，建立霓虹终端风格的完整 Web 界面，实现 Makefile 驱动的构建部署流程，并通过三层 E2E 测试确保系统稳定性。

## What Changes

### 前端通信协议
- From: 使用自定义 REST API 和 WebSocket 调用已删除的端点
- To: 使用官方 `@a2a-js/sdk` 对接 A2A 协议（JSON-RPC/SSE）
- Reason: 后端已全面转向 A2A 协议，旧端点已不存在
- Impact: Breaking change，前端需要完全重写通信层

### 前端设计系统
- From: 无统一设计系统，样式零散
- To: 霓虹终端设计系统（纯黑底 #0a0a1a + 霓虹绿 #00ff88 + 等宽字体 JetBrains Mono）
- Reason: 建立统一的视觉语言和组件库
- Impact: Non-breaking，新增设计 token 和组件样式

### 前端功能页面
- From: 仅聊天界面和侧边栏
- To: 8 个功能页面（聊天、工具、技能、设置、任务、记忆、作业、MCP）
- Reason: 覆盖 Synthia 全部核心功能
- Impact: Non-breaking，新增页面和路由

### 后端 CORS 配置
- From: 无 CORS 配置
- To: 新增 `CorsConfig` 结构体，默认允许 `http://localhost:5173`
- Reason: 支持前后端分离部署
- Impact: Non-breaking，新增配置项

### 工程化工具链
- From: 无 Makefile、无 Dockerfile
- To: Makefile（dev/build/test/deploy/docker/help）+ Dockerfile + docker-compose.yml
- Reason: 统一管理前后端构建、开发、部署流程
- Impact: Non-breaking，新增工程化文件

### 部署模式
- From: 无部署方案
- To: 前后端分离部署（Nginx 反向代理 + Rust 后端）
- Reason: 前端独立部署，后端专注 API 服务
- Impact: Non-breaking，新增部署配置

### E2E 测试体系
- From: 无 E2E 测试
- To: Playwright 三层测试（UI 交互、前后端联调、Agent 功能逻辑）
- Reason: 确保前后端联调和 Agent 功能正确性
- Impact: Non-breaking，新增测试框架和用例

## Capabilities

### New Capabilities
- `a2a-protocol-client`: 前端 A2A 协议客户端，使用 `@a2a-js/sdk` 实现消息发送、流式响应、任务管理
- `neon-terminal-design`: 霓虹终端设计系统，包含设计 token、基础组件、页面布局
- `web-feature-pages`: 8 个功能页面（聊天、工具、技能、设置、任务、记忆、作业、MCP）
- `cors-configuration`: 后端 CORS 配置，支持跨域请求
- `build-deployment-toolchain`: Makefile + Dockerfile + docker-compose，统一管理构建部署
- `e2e-testing-framework`: Playwright E2E 测试框架，覆盖三层测试

### Modified Capabilities
- `session-management`: 会话管理从旧 REST API 迁移到 A2A 协议
- `chat-interface`: 聊天界面从 WebSocket 迁移到 A2A SSE 流式响应

## Impact

**代码影响：**
- 前端：完全重写 `src/api/`、`src/components/`、`src/pages/`、`src/hooks/`
- 后端：新增 `crates/synthia-server/src/config/cors.rs`、修改 `router.rs` 添加 CORS Layer
- 根目录：新增 `Makefile`、`Dockerfile`、`docker-compose.yml`、`nginx.conf`

**API 影响：**
- 前端调用从 `/api/sessions` 迁移到 `/a2a/message:send` 和 `/a2a/tasks/{id}:subscribe`
- 后端新增 CORS 预检请求支持

**依赖影响：**
- 前端新增：`@a2a-js/sdk`、`react-router-dom`、`playwright`
- 后端新增：`tower-http`（CORS Layer）
- 开发环境新增：Playwright、Nginx

**系统影响：**
- 开发流程：`make dev` 一键启动前后端
- 部署流程：分离部署，Nginx 代理前端，后端专注 API
- 测试流程：`make test-e2e` 运行三层 E2E 测试
