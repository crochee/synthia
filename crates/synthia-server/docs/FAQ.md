---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 常见问题 (FAQ)

本文档收集了 Synthia Server 的常见问题和解答。

## 1. 目录

- [安装和配置](#安装和配置)
- [使用问题](#使用问题)
- [故障排查](#故障排查)
- [性能优化](#性能优化)
- [安全和权限](#安全和权限)

---

## 2. 安装和配置

> 详细的安装和配置说明请参考 [安装指南](getting-started/installation.md) 和 [配置说明](configuration/CONFIGURATION.md)。

### Q: 如何安装 Synthia Server？

A: 请参考 [安装指南](getting-started/installation.md) 获取详细的安装步骤。

### Q: 配置文件在哪里？

A: 默认配置文件路径：
- 配置文件：`./config.yaml`
- 数据库：`./.agents/synthia.db`
- 技能文件：`./.trae/skills/`

可以通过命令行参数或环境变量修改：

```bash
synthia-server --config /path/to/config.yaml
# 或
SYNTHIA_CONFIG=/path/to/config.yaml synthia-server
```

---

## 3. 使用问题

> 详细的使用说明请参考 [基本使用](getting-started/basic-usage.md) 和 [API使用指南](api-reference/API_GUIDE.md)。

### Q: 如何保持对话上下文？

A: 在后续请求中使用第一次聊天返回的 `session_id`。详见 [基本使用](getting-started/basic-usage.md#3-发送消息)。

### Q: 如何限制 Agent 的行为？

A: 通过配置文件设置 `allowed_tools` 和 `denied_tools`。详见 [配置说明](configuration/CONFIGURATION.md)。

### Q: 如何创建自定义工具？

A: 实现 `Tool` trait 并注册。详见 [工具开发指南](guides/tool-development.md)。

---

## 4. 故障排查

> 详细的故障排查指南请参考 [故障排查](operations/troubleshooting.md)。

### Q: 启动失败怎么办？

A: 请参考 [故障排查指南](operations/troubleshooting.md#3-常见问题) 获取详细的排查步骤。

### Q: 上下文超限怎么办？

A: 请参考 [故障排查指南](operations/troubleshooting.md#32-性能问题) 获取详细的解决方案。

---

## 5. 性能优化

> 详细的性能优化指南请参考 [性能优化](operations/performance-optimization.md)。

### Q: 如何提高响应速度？

A: 请参考 [性能优化指南](operations/performance-optimization.md) 获取详细的优化建议。

### Q: 如何处理高并发？

A: 请参考 [性能优化指南](operations/performance-optimization.md#3-并发控制) 获取详细的并发处理方案。

---

## 6. 安全和权限

> 详细的安全最佳实践请参考 [安全最佳实践](guides/security-best-practices.md)。

### Q: 如何保护 API Key？

A: 请参考 [安全最佳实践](guides/security-best-practices.md) 获取详细的安全配置建议。

### Q: 如何限制 Agent 的文件访问？

A: 通过配置限制工作目录。详见 [安全最佳实践](guides/security-best-practices.md)。

### Q: 如何审计 Agent 操作？

A: 启用日志记录：

```bash
# 启用详细日志
RUST_LOG=info,synthia_agent=debug synthia-server
```

---

## 7. 更多问题？

如果您的问题未在此列出，请：

1. 查看 [API 使用指南](api-reference/API_GUIDE.md)
2. 查看 [架构文档](architecture/ARCHITECTURE.md)
3. 在 [GitHub Issues](https://github.com/your-org/synthia/issues) 中搜索或提问
4. 加入我们的 [Discord](https://discord.gg/synthia) 社区
