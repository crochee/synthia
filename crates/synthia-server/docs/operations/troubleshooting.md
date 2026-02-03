---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 故障排查指南

## 1. 概述

本文档提供 Synthia Agent 常见问题的排查指南，包括诊断工具、常见问题和解决方案。

## 2. 诊断工具

### 2.1 健康检查

```bash
# 检查服务状态
curl http://localhost:8080/health

# 详细健康检查
curl http://localhost:8080/health/detail
```

**响应示例**：

```json
{
  "status": "healthy",
  "checks": {
    "database": {
      "status": "healthy",
      "latency_ms": 5
    },
    "redis": {
      "status": "healthy",
      "latency_ms": 2
    },
    "model_provider": {
      "status": "healthy",
      "latency_ms": 150
    }
  }
}
```

### 2.2 日志查看

```bash
# 查看实时日志
tail -f /var/log/synthia/app.log

# 查看错误日志
grep "ERROR" /var/log/synthia/app.log | tail -100

# 查看特定会话日志
grep "session_id=xxx" /var/log/synthia/app.log
```

### 2.3 指标查询

```bash
# 查询 Prometheus 指标
curl http://localhost:9090/api/v1/query?query=up

# 查询请求速率
curl 'http://localhost:9090/api/v1/query?query=rate(http_requests_total[5m])'

# 查询错误率
curl 'http://localhost:9090/api/v1/query?query=rate(http_requests_total{status=~"5.."}[5m])'
```

### 2.4 追踪查询

```bash
# Jaeger UI
open http://localhost:16686

# 查询特定追踪
curl 'http://localhost:16686/api/traces?service=synthia&limit=20'
```

## 3. 常见问题

### 3.1 启动失败

#### 问题：端口被占用

**症状**：
```
Error: Address already in use (os error 98)
```

**排查**：
```bash
# 检查端口占用
lsof -i :8080
netstat -tulpn | grep 8080
```

**解决**：
```bash
# 停止占用端口的进程
kill -9 <PID>

# 或修改配置使用其他端口
```

#### 问题：数据库连接失败

**症状**：
```
Error: Failed to connect to database: connection refused
```

**排查**：
```bash
# 检查数据库是否运行
systemctl status postgresql

# 检查数据库连接
psql -h localhost -U synthia -d synthia_db

# 检查网络连接
telnet localhost 5432
```

**解决**：
```bash
# 启动数据库
systemctl start postgresql

# 检查配置
cat config.yaml | grep database
```

#### 问题：配置文件错误

**症状**：
```
Error: Failed to parse config: missing field `api_key`
```

**排查**：
```bash
# 验证配置文件
synthia config validate

# 检查环境变量
env | grep SYNTHIA
```

**解决**：
```bash
# 修复配置文件
vim config.yaml

# 或设置环境变量
export SYNTHIA_API_KEY=xxx
```

### 3.2 性能问题

#### 问题：响应缓慢

**症状**：请求响应时间过长

**排查**：
```bash
# 检查系统资源
top
htop

# 检查内存使用
free -h

# 检查磁盘 I/O
iostat -x 1

# 检查网络延迟
ping api.openai.com
```

**解决**：
1. **增加资源**：增加 CPU 或内存
2. **优化配置**：调整并发数和缓存大小
3. **检查慢查询**：查看数据库慢查询日志

#### 问题：内存泄漏

**症状**：内存使用持续增长

**排查**：
```bash
# 监控内存使用
watch -n 1 'ps aux | grep synthia'

# 生成内存报告
curl http://localhost:8080/debug/memory

# 使用 valgrind 分析
valgrind --leak-check=full ./synthia-server
```

**解决**：
1. **重启服务**：临时解决
2. **修复代码**：定位并修复内存泄漏
3. **限制内存**：设置内存限制

#### 问题：上下文过长

**症状**：
```
Error: Context length exceeded
```

**排查**：
```bash
# 查看会话上下文大小
curl http://localhost:8080/sessions/{session_id}/context/stats
```

**解决**：
1. **启用压缩**：确保上下文压缩已启用
2. **调整阈值**：降低压缩触发阈值
3. **开始新会话**：开始新的会话

### 3.3 工具执行问题

#### 问题：工具超时

**症状**：
```
Error: Tool execution timeout
```

**排查**：
```bash
# 检查工具执行日志
grep "tool_timeout" /var/log/synthia/app.log

# 检查工具配置
curl http://localhost:8080/tools
```

**解决**：
1. **增加超时时间**：调整工具超时配置
2. **优化工具**：优化工具执行效率
3. **检查依赖**：检查工具依赖的服务

#### 问题：工具权限不足

**症状**：
```
Error: Permission denied
```

**排查**：
```bash
# 检查文件权限
ls -la /path/to/file

# 检查用户权限
whoami
groups
```

**解决**：
```bash
# 修改文件权限
chmod 644 /path/to/file

# 修改文件所有者
chown synthia:synthia /path/to/file
```

#### 问题：工具循环

**症状**：工具重复执行相同操作

**排查**：
```bash
# 查看工具调用历史
curl http://localhost:8080/sessions/{session_id}/tools/history

# 检查循环检测日志
grep "loop_detected" /var/log/synthia/app.log
```

**解决**：
1. **调整循环检测阈值**：降低检测阈值
2. **优化提示**：改进系统提示避免循环
3. **手动干预**：发送 steering 消息中断

### 3.4 模型调用问题

#### 问题：API 密钥无效

**症状**：
```
Error: Invalid API key
```

**排查**：
```bash
# 检查 API 密钥配置
cat config.yaml | grep api_key

# 测试 API 密钥
curl -H "Authorization: Bearer $API_KEY" \
  https://api.openai.com/v1/models
```

**解决**：
```bash
# 更新 API 密钥
export OPENAI_API_KEY=sk-xxx

# 或修改配置文件
vim config.yaml
```

#### 问题：配额超限

**症状**：
```
Error: Rate limit exceeded
```

**排查**：
```bash
# 检查配额使用
curl https://api.openai.com/v1/usage \
  -H "Authorization: Bearer $API_KEY"
```

**解决**：
1. **等待重置**：等待配额重置
2. **升级计划**：升级 API 计划
3. **使用其他模型**：切换到其他模型提供商

#### 问题：模型响应异常

**症状**：模型返回空响应或格式错误

**排查**：
```bash
# 查看模型调用日志
grep "model_call" /var/log/synthia/app.log | tail -20

# 测试模型
curl http://localhost:8080/test/model \
  -H "Content-Type: application/json" \
  -d '{"message": "test"}'
```

**解决**：
1. **重试请求**：自动重试机制
2. **切换模型**：使用备用模型
3. **调整参数**：调整温度等参数

### 3.5 网络问题

#### 问题：连接超时

**症状**：
```
Error: Connection timeout
```

**排查**：
```bash
# 检查网络连接
ping api.openai.com
traceroute api.openai.com

# 检查防火墙
iptables -L
```

**解决**：
1. **检查网络**：确保网络畅通
2. **配置代理**：如需要，配置代理
3. **增加超时**：增加连接超时时间

#### 问题：SSL 证书错误

**症状**：
```
Error: SSL certificate verify failed
```

**排查**：
```bash
# 检查证书
openssl s_client -connect api.openai.com:443

# 检查系统证书
ls -la /etc/ssl/certs/
```

**解决**：
```bash
# 更新证书
update-ca-certificates

# 或禁用证书验证（不推荐）
export SSL_CERT_FILE=/dev/null
```

## 4. 调试技巧

### 4.1 启用调试模式

```yaml
logging:
  level: debug
  
debug:
  enabled: true
  endpoints:
    - /debug/pprof
    - /debug/vars
```

### 4.2 使用调试端点

```bash
# CPU profiling
curl http://localhost:8080/debug/pprof/profile?seconds=30 > cpu.prof

# Heap profiling
curl http://localhost:8080/debug/pprof/heap > heap.prof

# Goroutine dump
curl http://localhost:8080/debug/pprof/goroutine > goroutine.prof

# 分析 profile
go tool pprof cpu.prof
```

### 4.3 远程调试

```bash
# 启用远程调试
synthia-server --debug-addr 0.0.0.0:4000

# 连接调试器
dlv connect localhost:4000
```

## 5. 日志分析

### 5.1 关键日志模式

```bash
# 错误日志
grep "ERROR" /var/log/synthia/app.log

# 警告日志
grep "WARN" /var/log/synthia/app.log

# 特定会话
grep "session_id=xxx" /var/log/synthia/app.log

# 特定工具
grep "tool_name=read" /var/log/synthia/app.log

# 慢请求
grep "duration_ms=[0-9]{4,}" /var/log/synthia/app.log
```

### 5.2 日志统计

```bash
# 错误统计
grep "ERROR" /var/log/synthia/app.log | cut -d' ' -f5 | sort | uniq -c | sort -rn

# 请求统计
grep "request" /var/log/synthia/app.log | wc -l

# 响应时间分布
grep "duration_ms" /var/log/synthia/app.log | \
  awk -F'duration_ms=' '{print $2}' | \
  awk '{sum+=$1; count++} END {print "avg:", sum/count, "total:", count}'
```

## 6. 应急处理

### 6.1 服务重启

```bash
# 优雅重启
systemctl reload synthia

# 强制重启
systemctl restart synthia
```

### 6.2 回滚版本

```bash
# 回滚到上一版本
synthia rollback

# 回滚到指定版本
synthia rollback --version v1.2.3
```

### 6.3 紧急停止

```bash
# 停止服务
systemctl stop synthia

# 强制停止
kill -9 $(pgrep synthia)
```

## 7. 预防措施

### 7.1 定期检查

```bash
# 每日检查脚本
#!/bin/bash
# 检查服务状态
systemctl is-active synthia

# 检查磁盘空间
df -h | grep -E '^/dev'

# 检查内存使用
free -h | grep Mem

# 检查错误日志
grep "ERROR" /var/log/synthia/app.log | tail -10
```

### 7.2 自动化监控

```yaml
monitoring:
  automated_checks:
    - name: service_health
      interval: 60s
      command: curl -f http://localhost:8080/health
    
    - name: disk_space
      interval: 300s
      command: df -h | grep -E '^/dev' | awk '{if ($5 > 80) exit 1}'
    
    - name: memory_usage
      interval: 60s
      command: free | grep Mem | awk '{if ($3/$2 > 0.9) exit 1}'
```

### 7.3 备份策略

```bash
# 备份数据库
pg_dump synthia_db > backup_$(date +%Y%m%d).sql

# 备份配置
tar -czf config_backup_$(date +%Y%m%d).tar.gz config.yaml

# 备份日志
tar -czf logs_backup_$(date +%Y%m%d).tar.gz /var/log/synthia/
```

## 8. 相关文档

- [错误恢复](../guides/error-recovery.md)
- [监控告警](monitoring-alerting.md)
- [性能优化](performance-optimization.md)

## 9. 参考资料

- [Rust Debugging](https://rust-lang.github.io/async-book/06_multiple_futures/01_chapter.html)
- [PostgreSQL Troubleshooting](https://www.postgresql.org/docs/current/maintenance.html)
- [Linux Performance Analysis](https://brendangregg.com/linuxperf.html)
