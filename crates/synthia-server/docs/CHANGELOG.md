---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 更新日志

本项目的所有重要变更都将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增
- 待发布的新功能

## [0.1.0] - 2024-01-15

### 新增
- 初始版本发布
- 核心 Agent 执行引擎 (ReAct 模式)
- REST API 服务
  - `/health` - 健康检查
  - `/chat` - 同步聊天
  - `/chat/stream` - 流式聊天 (SSE)
  - `/sessions` - 会话管理
  - `/tools` - 工具管理
  - `/skills` - 技能管理
  - `/mcp` - MCP 服务器管理
  - `/models` - 模型提供商管理
- WebSocket 支持
  - 实时双向通信
  - 会话隔离
  - 取消执行
- 工具系统
  - 内置文件系统工具
  - 内置 Web 工具
  - 自定义工具支持
  - 工具过滤 (白名单/黑名单)
  - 并发执行
- 技能系统
  - Markdown 格式技能文件
  - 动态加载/卸载
  - 技能注入
- MCP 集成
  - stdio 传输
  - SSE 传输
  - HTTP 传输
  - 动态注册/注销
- 存储系统
  - SQLite 持久化
  - 会话历史
  - 消息存储
- 上下文管理
  - 自动压缩
  - Token 计数
  - 上下文窗口监控
- 错误恢复
  - 自动重试
  - 指数退避
  - 循环检测
  - 降级处理
- 认证授权
  - API Key 认证
  - Bearer Token 格式
- 配置管理
  - YAML 配置文件
  - 运行时配置更新
  - 多模型提供商支持

### 安全
- 路径验证防止目录遍历
- 输入参数验证
- 敏感信息保护

---

## 版本说明

### 版本号格式

- **主版本号 (MAJOR)**: 不兼容的 API 变更
- **次版本号 (MINOR)**: 向后兼容的功能新增
- **修订号 (PATCH)**: 向后兼容的问题修复

### 变更类型

- **新增 (Added)**: 新功能
- **变更 (Changed)**: 现有功能的变更
- **弃用 (Deprecated)**: 即将移除的功能
- **移除 (Removed)**: 已移除的功能
- **修复 (Fixed)**: Bug 修复
- **安全 (Security)**: 安全相关的修复
