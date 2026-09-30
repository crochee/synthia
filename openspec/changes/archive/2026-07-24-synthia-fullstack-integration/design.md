## Context

Synthia 是一个 AI Agent 平台，当前后端已重构为 A2A 协议驱动，但前端仍使用旧的 REST API 和 WebSocket 端点，导致前后端严重脱节。项目缺乏统一的工程化管理（Makefile、Dockerfile）、设计系统和 E2E 测试体系。

**现状痛点：**
- 前端调用已删除的 `/api/sessions` 和 WebSocket 端点
- 无设计系统，UI 风格不统一
- 无工程化工具链，开发部署流程不清晰
- 无 E2E 测试，无法验证前后端联调
- 用户希望整体一次性设计，打通前后端联调

**约束条件：**
- 必须使用官方 `@a2a-js/sdk` 对接 A2A 协议
- 采用霓虹终端设计风格（纯黑底 + 霓虹绿/青 + 等宽字体）
- 前后端分离部署（Nginx 反向代理）
- 并行开发模式，三条轨道独立推进

## Goals / Non-Goals

**Goals:**
1. 前端完全迁移到 A2A 协议，使用 `@a2a-js/sdk`
2. 建立霓虹终端设计系统，统一 UI 风格
3. 实现 P0+P1+P2 全部功能页面（聊天、工具、技能、设置、任务、记忆、作业、MCP）
4. 建立 Makefile 工程化体系，支持开发、构建、测试、部署
5. 实现三层 E2E 测试（UI 交互、前后端联调、Agent 功能逻辑）
6. 前后端分离部署，Nginx 反向代理

**Non-Goals:**
- 不实现单体部署模式（前端嵌入后端二进制）
- 不实现国际化（i18n）
- 不实现用户认证和权限系统
- 不实现移动端适配（仅桌面端）

## Decisions

### D1: 使用 `@a2a-js/sdk` 对接 A2A 协议

**选择：** 安装并使用官方 `@a2a-js/sdk` 作为 A2A 协议客户端

**理由：**
- 官方 SDK v1.0 稳定版，支持 JSON-RPC、HTTP+JSON/REST、gRPC 三种传输
- 减少自定义实现，降低维护成本
- 与后端 A2A 协议实现保持一致

**已考虑 alternative：**
- 自定义 A2A 客户端：增加维护成本，容易与后端实现不一致
- 使用其他 A2A SDK：官方 SDK 更稳定，社区支持更好

### D2: 霓虹终端设计风格

**选择：** 纯黑底（`#0a0a1a`）+ 霓虹绿（`#00ff88`）+ 青色（`#00ddff`）+ 等宽字体（JetBrains Mono）

**理由：**
- 符合开发者审美，科技感强
- 高对比度，视觉舒适
- 与 Synthia 的 AI/技术定位匹配

**已考虑 alternative：**
- 深空科幻（GitHub Dark 色调 + 蓝色强调）：不够独特
- 玻璃未来（深紫渐变 + 毛玻璃）：过于花哨，影响可读性
- 当前风格精修：缺乏统一设计系统

### D3: 前后端分离部署

**选择：** 前端通过 Nginx 反向代理提供服务，后端仅服务 API

**理由：**
- 前后端独立扩展，部署灵活
- 前端可使用 CDN 加速
- 后端专注于 API 服务，职责清晰

**已考虑 alternative：**
- 单体部署（前端嵌入后端二进制）：部署简单，但扩展性差
- 混合模式：增加复杂度，收益不明显

**Nginx 配置要点：**
- 处理 SSE streaming（`proxy_buffering off`）
- 代理 `/api` 和 `/a2a` 到后端
- 静态文件缓存策略

### D4: 并行开发模式

**选择：** 前端轨道、后端轨道、集成轨道并行进行

**理由：**
- 前后端独立开发，互不阻塞
- 加快整体进度
- 集成轨道最后进行，确保前后端稳定

**已考虑 alternative：**
- 渐进式迁移：周期长，中间状态多
- 分层迭代：依赖关系复杂，容易卡住

### D5: 三层 E2E 测试体系

**选择：** 使用 Playwright 实现三层测试（UI 交互、前后端联调、Agent 功能逻辑）

**理由：**
- UI 交互测试：验证页面渲染、组件交互
- 前后端联调测试：验证 A2A 协议流程、API 调用
- Agent 功能逻辑测试：验证端到端对话、工具调用、任务生命周期

**已考虑 alternative：**
- 仅 UI 测试：无法验证前后端联调
- 仅集成测试：无法验证 UI 交互细节
- 使用其他测试框架：Playwright 支持多浏览器，生态成熟

### D6: A2A SDK 连接状态语义

**选择：** 使用 `isServerAvailable` 而非 `isConnected`，通过 `/health` 端点定期检测

**理由：**
- A2A SDK 使用 HTTP/SSE 而非 WebSocket，无持久连接
- 通过健康检查端点检测服务器可用性更准确
- 每 30 秒轮询一次，平衡实时性和性能

**已考虑 alternative：**
- 使用 WebSocket 连接状态：A2A 协议不支持
- 不检测连接状态：用户体验差，无法感知服务器状态

### D7: Vite 代理配置

**选择：** 同时代理 `/api` 和 `/a2a` 到后端

**理由：**
- 开发模式前端（:5173）和后端（:8080）端口分离
- 避免跨域问题
- 简化开发环境配置

**已考虑 alternative：**
- 仅代理 `/api`：A2A 协议端点无法访问
- 使用 CORS：增加配置复杂度

### D8: Docker 多阶段构建

**选择：** Rust 镜像使用 `rust:1.95-alpine`，前端镜像使用 `nginx:alpine`

**理由：**
- Alpine 镜像体积小
- 多阶段构建减少最终镜像大小
- 开发环境使用独立的 `Dockerfile.server.dev` 和 `Dockerfile.web.dev`

**已考虑 alternative：**
- 使用完整 Debian 镜像：体积过大
- 单阶段构建：镜像包含构建工具，体积大

### D9: 后端 CORS 配置

**选择：** 新增 `CorsConfig` 结构体到 `ServerConfig`，默认允许 `http://localhost:5173`

**理由：**
- 支持前后端分离部署
- 使用 `tower-http` 的 `CorsLayer`，与 Axum 框架集成良好
- 可配置化，支持不同环境

**已考虑 alternative：**
- 硬编码 CORS 规则：不灵活
- 不使用 CORS：前后端分离部署无法工作

### D10: 移除静态文件嵌入

**选择：** 分离部署模式下后端不再嵌入前端产物，移除 `embedded-ui` feature 和 `rust-embed` 依赖

**理由：**
- 前后端分离部署，后端不需要服务静态文件
- 减少后端依赖和复杂度
- 前端由 Nginx 提供服务，性能更好

**已考虑 alternative：**
- 保留静态文件嵌入：增加后端复杂度，与分离部署模式冲突

## Risks / Trade-offs

**Risk 1: A2A SDK 版本兼容性**
- 描述：`@a2a-js/sdk` 可能与后端 A2A 协议实现不完全兼容
- Mitigation：锁定 SDK 版本，提前进行协议兼容性测试

**Risk 2: 并行开发集成风险**
- 描述：前后端独立开发，集成时可能发现协议不匹配
- Mitigation：定义清晰的 API 契约，早期进行集成测试

**Risk 3: 设计系统一致性**
- 描述：霓虹终端设计风格在多个页面可能不一致
- Mitigation：建立设计 token 和组件库，统一样式管理

**Risk 4: E2E 测试稳定性**
- 描述：E2E 测试可能因网络、环境等因素不稳定
- Mitigation：使用重试机制，稳定化测试选择器

**Risk 5: Docker 构建复杂度**
- 描述：多阶段构建可能增加构建时间和复杂度
- Mitigation：优化 Dockerfile，使用构建缓存

**Trade-off 1: 分离部署 vs 单体部署**
- 取舍：分离部署增加部署复杂度，但提供更好的扩展性和灵活性
- 接受理由：前后端独立扩展，符合现代微服务架构趋势

**Trade-off 2: 并行开发 vs 渐进式迁移**
- 取舍：并行开发速度快，但集成风险高
- 接受理由：用户要求整体一次性设计，并行开发更符合需求

**Trade-off 3: 三层 E2E 测试 vs 单层测试**
- 取舍：三层测试覆盖全面，但测试数量多，维护成本高
- 接受理由：用户要求全部覆盖，确保前后端联调和 Agent 功能逻辑正确

## Migration Plan

**部署顺序：**

1. **Phase 1: Foundation**
   - 后端：CORS 配置
   - 前端：切换到 `@a2a-js/sdk`，最小集成
   - 工程化：Makefile（dev/build/test 目标）、Dockerfiles
   - 验证：`make dev` 启动前后端，能发送消息

2. **Phase 2: Design + Core**
   - 前端：霓虹终端设计 token、React Router 设置
   - 前端：重构聊天界面（完整 A2A 流程）
   - 前端：重构会话管理
   - 验证：核心聊天体验完整，视觉统一

3. **Phase 3: Management**
   - 前端：工具面板 + 技能面板
   - 前端：设置页面（Provider/模型配置）
   - 前端：任务面板 + 记忆搜索
   - 前端：作业调度 + MCP 管理
   - 验证：所有管理功能可用

4. **Phase 4: Testing**
   - 工程化：Playwright 集成 + Page Object Model
   - 测试：层级 1 UI 交互测试
   - 测试：层级 2 前后端联调测试
   - 测试：层级 3 Agent 功能逻辑测试
   - 验证：三层测试全部通过

5. **Phase 5: Deployment**
   - 工程化：Docker Compose 生产配置
   - 工程化：Nginx 反向代理配置
   - 验证：分离部署工作正常
   - 文档：README 更新，部署指南

**Rollback 策略：**
- 每个 Phase 完成后进行代码审查和测试
- 使用 Git 分支管理，支持快速回滚
- 关键配置使用环境变量，支持动态调整

**验收条件：**
- 所有功能页面可用
- 三层 E2E 测试全部通过
- 分离部署工作正常
- 文档完整

## Open Questions

1. **A2A SDK 版本选择**：`@a2a-js/sdk` 的具体版本需要确认，是否需要锁定到特定版本？

2. **Nginx 配置细节**：SSE streaming 的具体配置参数需要验证，是否需要额外的超时设置？

3. **E2E 测试环境**：Playwright 测试是否需要独立的后端实例，还是使用开发环境？

4. **设计 token 管理**：是否需要使用 CSS-in-JS 或其他设计 token 管理工具？

5. **Makefile 命令粒度**：是否需要更细粒度的命令（如 `make test-ui`、`make test-integration`）？

6. **Docker 镜像仓库**：构建的 Docker 镜像是否需要推送到特定仓库？

7. **CI/CD 集成**：是否需要配置 GitHub Actions 或其他 CI/CD 工具？

8. **性能优化**：前端是否需要实现代码分割、懒加载等性能优化？
