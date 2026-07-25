## 1. Phase 1: Foundation

- [x] 1.1 后端：实现 CORS 配置模块（CorsConfig 结构体、tower-http CorsLayer 集成）
- [x] 1.2 后端：添加 CORS 配置到 ServerConfig，默认允许 http://localhost:5173
- [x] 1.3 前端：安装 @a2a-js/sdk 依赖
- [x] 1.4 前端：创建 A2A 客户端封装模块（api/a2a-client.ts）
- [x] 1.5 前端：实现最小化 A2A 消息发送功能（sendMessage）
- [x] 1.6 前端：实现最小化 A2A 流式响应接收（sendMessageStream）
- [x] 1.7 工程化：创建根目录 Makefile（dev/build/test/deploy/docker/help 命令）
- [x] 1.8 工程化：创建 Dockerfile.server（Rust 1.95-alpine 多阶段构建）
- [x] 1.9 工程化：创建 Dockerfile.web（Node 20-alpine 构建前端）
- [x] 1.10 验证：make dev 启动前后端，能发送消息并收到响应

## 2. Phase 2: Design + Core

- [x] 2.1 前端：创建霓虹终端设计 token（styles/tokens.css：颜色、字体、间距、圆角）
- [x] 2.2 前端：实现基础 UI 组件库（Button、Input、Card、Modal 等）
- [x] 2.3 前端：安装并配置 React Router v6
- [x] 2.4 前端：实现应用布局组件（Header、Sidebar、MainContent）
- [x] 2.5 前端：重构聊天界面组件（ChatPage.tsx）
- [x] 2.6 前端：实现完整 A2A 协议流程（sendTask → SSE streaming → completed）
- [x] 2.7 前端：重构会话管理功能（创建/列表/删除/切换会话）
- [x] 2.8 前端：实现连接状态指示器（isServerAvailable 健康检查）
- [x] 2.9 验证：核心聊天体验完整，视觉风格统一为霓虹终端

## 3. Phase 3: Management Features

- [x] 3.1 前端：实现工具面板页面（ToolsPage.tsx，查看可用工具列表）
- [x] 3.2 前端：实现技能面板页面（SkillsPage.tsx，查看/管理已注册技能）
- [x] 3.3 前端：实现设置页面（SettingsPage.tsx，Provider 配置、模型选择）
- [x] 3.4 前端：实现任务面板页面（TasksPage.tsx，查看 A2A 任务历史和状态）
- [x] 3.5 前端：实现记忆搜索页面（MemoryPage.tsx，搜索 agent 记忆）
- [x] 3.6 前端：实现作业调度页面（JobsPage.tsx，查看/管理定时任务）
- [x] 3.7 前端：实现 MCP 管理页面（McpPage.tsx，MCP 服务器注册和管理）
- [x] 3.8 验证：所有管理功能页面可访问且功能正常

## 4. Phase 4: Testing

- [x] 4.1 工程化：安装 Playwright 并创建 playwright.config.ts
- [x] 4.2 工程化：创建 Page Object Model 基础类（base.page.ts）
- [x] 4.3 工程化：创建聊天页面对象（chat.page.ts）
- [x] 4.4 工程化：创建工具/技能/设置页面对象
- [x] 4.5 测试：实现层级 1 UI 交互测试（页面导航、组件渲染、表单输入）
- [x] 4.6 测试：实现层级 1 UI 交互测试（聊天消息发送/接收、会话管理）
- [x] 4.7 测试：实现层级 2 前后端联调测试（A2A 协议完整流程）
- [x] 4.8 测试：实现层级 2 前后端联调测试（管理 API CRUD 操作）
- [x] 4.9 测试：实现层级 3 Agent 功能逻辑测试（端到端对话、工具调用）
- [x] 4.10 测试：实现层级 3 Agent 功能逻辑测试（任务生命周期、审批流程）
- [x] 4.11 验证：三层 E2E 测试全部通过

## 5. Phase 5: Deployment

- [x] 5.1 工程化：创建 docker-compose.yml（开发环境配置）
- [x] 5.2 工程化：创建 docker-compose.prod.yml（生产环境配置）
- [x] 5.3 工程化：创建 Nginx 反向代理配置（nginx.conf）
- [x] 5.4 工程化：配置 Nginx SSE streaming 支持（proxy_buffering off）
- [x] 5.5 验证：Docker Compose 开发环境启动正常
- [x] 5.6 验证：Docker Compose 生产环境部署正常（前后端分离）
- [x] 5.7 文档：更新 README.md（安装、开发、部署说明）
- [x] 5.8 文档：创建部署指南（DEPLOYMENT.md）
