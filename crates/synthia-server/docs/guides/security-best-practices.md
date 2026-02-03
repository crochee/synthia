---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 安全最佳实践

## 1. 概述

安全性是 Synthia Agent 设计的核心考虑。本文档说明安全威胁模型、安全配置、敏感信息保护和安全最佳实践。

## 2. 安全威胁模型

### 2.1 威胁分类

```
┌─────────────────────────────────────────────────────────────┐
│                      Security Threats                        │
│                                                              │
│  1. 数据泄露                                                 │
│     ├── 敏感信息暴露                                         │
│     ├── API密钥泄露                                          │
│     └── 用户数据泄露                                         │
│                                                              │
│  2. 未授权访问                                               │
│     ├── 文件系统访问                                         │
│     ├── 网络访问                                             │
│     └── 系统命令执行                                         │
│                                                              │
│  3. 恶意输入                                                 │
│     ├── Prompt注入                                           │
│     ├── 路径遍历                                             │
│     └── 命令注入                                             │
│                                                              │
│  4. 资源滥用                                                 │
│     ├── Token消耗                                            │
│     ├── 计算资源                                             │
│     └── 网络带宽                                             │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 威胁矩阵

| 威胁 | 影响 | 可能性 | 缓解措施 |
|------|------|--------|----------|
| API密钥泄露 | 高 | 中 | 环境变量、密钥管理 |
| 文件系统破坏 | 高 | 低 | 沙箱、权限控制 |
| Prompt注入 | 中 | 高 | 输入验证、输出过滤 |
| 资源耗尽 | 中 | 中 | 限制、监控 |

## 3. 安全配置

### 3.1 访问控制

```yaml
security:
  # 工作空间限制
  workspace:
    allowed_paths:
      - "/home/user/projects"
      - "/home/user/documents"
    denied_paths:
      - "/etc"
      - "/root"
      - "**/.ssh"
      - "**/.env"
  
  # 工具权限
  tools:
    allowed:
      - read
      - write
      - grep
      - glob
    denied:
      - exec
      - delete
  
  # 网络访问
  network:
    allowed_domains:
      - "api.openai.com"
      - "api.anthropic.com"
    denied_domains:
      - "*"
    max_request_size: 10MB
    timeout: 30s
```

### 3.2 认证配置

```yaml
auth:
  enabled: true
  type: "jwt"  # jwt, api_key, oauth
  
  jwt:
    secret: "${JWT_SECRET}"
    expiration: "24h"
    issuer: "synthia"
  
  api_key:
    header: "X-API-Key"
    keys:
      - name: "admin"
        key: "${ADMIN_API_KEY}"
        permissions: ["*"]
      - name: "readonly"
        key: "${READONLY_API_KEY}"
        permissions: ["read", "list"]
```

### 3.3 审计配置

```yaml
audit:
  enabled: true
  log_file: "/var/log/synthia/audit.log"
  
  events:
    - tool_call
    - file_access
    - auth_attempt
    - config_change
  
  retention: "30d"
  max_size: "100MB"
```

## 4. 敏感信息保护

### 4.1 环境变量

```bash
# 不要在代码中硬编码密钥
# 不好的做法
api_key = "sk-1234567890abcdef"

# 好的做法：使用环境变量
api_key = std::env::var("OPENAI_API_KEY")
    .expect("OPENAI_API_KEY must be set");
```

### 4.2 密钥管理

```rust
use secrecy::{Secret, ExposeSecret};

pub struct ApiConfig {
    pub openai_api_key: Secret<String>,
    pub anthropic_api_key: Secret<String>,
}

impl ApiConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            openai_api_key: Secret::new(
                std::env::var("OPENAI_API_KEY")?
            ),
            anthropic_api_key: Secret::new(
                std::env::var("ANTHROPIC_API_KEY")?
            ),
        })
    }
    
    pub fn get_openai_key(&self) -> &str {
        self.openai_api_key.expose_secret()
    }
}

// 避免意外打印
println!("API Key: {}", config.openai_api_key);  // 编译错误
```

### 4.3 日志脱敏

```rust
use tracing::info;

pub fn log_tool_call(tool_name: &str, args: &Value) {
    // 脱敏敏感参数
    let sanitized_args = sanitize_sensitive_data(args);
    
    info!(
        tool_name = %tool_name,
        args = ?sanitized_args,
        "Tool called"
    );
}

fn sanitize_sensitive_data(args: &Value) -> Value {
    let mut sanitized = args.clone();
    
    if let Some(obj) = sanitized.as_object_mut() {
        for key in ["password", "api_key", "token", "secret"] {
            if obj.contains_key(key) {
                obj.insert(key.to_string(), json!("***REDACTED***"));
            }
        }
    }
    
    sanitized
}
```

### 4.4 文件权限

```rust
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;

pub fn create_secure_file(path: &Path, content: &str) -> Result<()> {
    // 创建文件
    fs::write(path, content)?;
    
    // 设置权限：仅所有者可读写
    fs::set_permissions(
        path,
        Permissions::from_mode(0o600)
    )?;
    
    Ok(())
}
```

## 5. 输入验证

### 5.1 路径验证

```rust
pub fn validate_path(base: &Path, path: &str) -> Result<PathBuf> {
    let full_path = base.join(path);
    
    // 规范化路径
    let canonical = full_path.canonicalize()
        .map_err(|_| AgentError::invalid_input("Invalid path"))?;
    
    // 检查是否在允许的目录内
    let base_canonical = base.canonicalize()
        .map_err(|_| AgentError::internal("Invalid base path"))?;
    
    if !canonical.starts_with(&base_canonical) {
        return Err(AgentError::permission_denied(
            "Path outside allowed directory"
        ));
    }
    
    // 检查路径遍历
    if path.contains("..") {
        return Err(AgentError::invalid_input(
            "Path traversal not allowed"
        ));
    }
    
    Ok(canonical)
}
```

### 5.2 命令验证

```rust
pub fn validate_command(command: &str) -> Result<()> {
    // 黑名单检查
    let blacklist = [
        "rm -rf",
        "dd if=",
        "mkfs",
        ":(){ :|:& };:",
        "chmod 777",
    ];
    
    for pattern in &blacklist {
        if command.contains(pattern) {
            return Err(AgentError::invalid_input(
                format!("Dangerous command pattern: {}", pattern)
            ));
        }
    }
    
    // 白名单检查
    let allowed_commands = ["ls", "cat", "grep", "find"];
    let cmd = command.split_whitespace().next().unwrap_or("");
    
    if !allowed_commands.contains(&cmd) {
        return Err(AgentError::invalid_input(
            format!("Command not allowed: {}", cmd)
        ));
    }
    
    Ok(())
}
```

### 5.3 Prompt注入防护

```rust
pub fn sanitize_user_input(input: &str) -> String {
    // 移除控制字符
    let sanitized: String = input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect();
    
    // 检测注入模式
    let injection_patterns = [
        "ignore previous instructions",
        "disregard all above",
        "system:",
        "assistant:",
    ];
    
    let lower = sanitized.to_lowercase();
    for pattern in &injection_patterns {
        if lower.contains(pattern) {
            tracing::warn!(
                pattern = %pattern,
                "Potential prompt injection detected"
            );
        }
    }
    
    sanitized
}
```

## 6. 沙箱隔离

### 6.1 文件系统沙箱

```rust
use sandbox::FileSystemSandbox;

pub struct SecureToolExecutor {
    sandbox: FileSystemSandbox,
}

impl SecureToolExecutor {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            sandbox: FileSystemSandbox::new(workspace)
                .readonly(["/etc", "/usr"])
                .deny(["**/.ssh", "**/.env", "**/secrets"])
                .max_file_size(10 * 1024 * 1024)  // 10MB
                .build(),
        }
    }
    
    pub async fn read_file(&self, path: &str) -> Result<String> {
        // 沙箱自动验证路径
        let content = self.sandbox.read_file(path).await?;
        Ok(content)
    }
}
```

### 6.2 网络沙箱

```rust
use sandbox::NetworkSandbox;

pub struct SecureWebClient {
    sandbox: NetworkSandbox,
}

impl SecureWebClient {
    pub fn new() -> Self {
        Self {
            sandbox: NetworkSandbox::new()
                .allow_domains(["api.openai.com", "api.anthropic.com"])
                .deny_domains(["*"])
                .max_response_size(10 * 1024 * 1024)  // 10MB
                .timeout(Duration::from_secs(30))
                .build(),
        }
    }
    
    pub async fn fetch(&self, url: &str) -> Result<String> {
        // 沙箱自动验证URL
        let content = self.sandbox.fetch(url).await?;
        Ok(content)
    }
}
```

## 7. 监控和告警

### 7.1 安全监控

```rust
pub struct SecurityMonitor {
    events: Vec<SecurityEvent>,
}

#[derive(Debug)]
pub struct SecurityEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: SecurityEventType,
    pub severity: Severity,
    pub details: String,
}

#[derive(Debug)]
pub enum SecurityEventType {
    UnauthorizedAccess,
    SuspiciousInput,
    RateLimitExceeded,
    PermissionDenied,
    AnomalousBehavior,
}

impl SecurityMonitor {
    pub fn check(&self) -> Vec<SecurityAlert> {
        let mut alerts = Vec::new();
        
        // 检查异常行为
        let recent_events: Vec<_> = self.events
            .iter()
            .filter(|e| e.timestamp > Utc::now() - Duration::from_secs(3600))
            .collect();
        
        // 检查频繁的权限拒绝
        let denied_count = recent_events
            .iter()
            .filter(|e| matches!(e.event_type, SecurityEventType::PermissionDenied))
            .count();
        
        if denied_count > 10 {
            alerts.push(SecurityAlert {
                severity: Severity::High,
                message: format!("{} permission denied events in last hour", denied_count),
            });
        }
        
        alerts
    }
}
```

### 7.2 告警配置

```yaml
alerts:
  channels:
    - type: "email"
      recipients: ["security@example.com"]
    
    - type: "slack"
      webhook: "${SLACK_WEBHOOK_URL}"
  
  rules:
    - name: "high_rate_limit"
      condition: "rate_limit_exceeded > 100 in 5m"
      severity: "high"
    
    - name: "suspicious_input"
      condition: "suspicious_input > 10 in 1h"
      severity: "medium"
    
    - name: "unauthorized_access"
      condition: "unauthorized_access > 5 in 1h"
      severity: "critical"
```

## 8. 安全最佳实践

### 8.1 最小权限原则

```rust
// 不好的做法：给予所有权限
let agent = Agent::new()
    .allow_all_tools()
    .allow_all_paths();

// 好的做法：仅给予必要权限
let agent = Agent::new()
    .allow_tools(["read", "grep"])
    .allow_paths(["/home/user/project"])
    .deny_tools(["exec", "delete"]);
```

### 8.2 防御深度

```rust
// 多层防御
pub async fn execute_tool_secure(
    tool_name: &str,
    args: &Value,
    context: &ToolContext,
) -> Result<ToolResult> {
    // 第一层：输入验证
    validate_tool_input(tool_name, args)?;
    
    // 第二层：权限检查
    check_permission(tool_name, &context.permissions)?;
    
    // 第三层：沙箱执行
    let result = execute_in_sandbox(tool_name, args).await?;
    
    // 第四层：输出过滤
    let filtered = filter_sensitive_output(result)?;
    
    Ok(filtered)
}
```

### 8.3 安全审计

```rust
pub struct SecurityAuditor {
    audit_log: AuditLog,
}

impl SecurityAuditor {
    pub fn audit_tool_call(
        &self,
        tool_name: &str,
        args: &Value,
        result: &ToolResult,
    ) {
        self.audit_log.record(AuditEntry {
            timestamp: Utc::now(),
            user_id: get_current_user_id(),
            session_id: get_current_session_id(),
            action: format!("tool_call:{}", tool_name),
            args: sanitize_for_audit(args),
            result: sanitize_for_audit(result),
            ip_address: get_client_ip(),
        });
    }
}
```

### 8.4 定期安全检查

```bash
#!/bin/bash
# 安全检查脚本

# 检查敏感文件权限
find . -name "*.env" -exec chmod 600 {} \;
find . -name "*key*" -exec chmod 600 {} \;

# 检查日志中的敏感信息
grep -r "password\|api_key\|secret" /var/log/synthia/ | wc -l

# 检查依赖漏洞
cargo audit

# 检查配置文件权限
ls -la config/
```

## 9. 应急响应

### 9.1 应急响应流程

```
┌─────────────────────────────────────────────────────────────┐
│                Incident Response Flow                        │
│                                                              │
│  1. 检测安全事件                                             │
│     │                                                        │
│     ▼                                                        │
│  2. 评估影响                                                 │
│     ├── 低：记录并监控                                       │
│     ├── 中：调查并修复                                       │
│     └── 高：立即响应                                         │
│     │                                                        │
│     ▼                                                        │
│  3. 遏制措施                                                 │
│     ├── 禁用受影响功能                                       │
│     ├── 隔离受影响系统                                       │
│     └── 撤销受影响凭证                                       │
│     │                                                        │
│     ▼                                                        │
│  4. 根除威胁                                                 │
│     ├── 修复漏洞                                             │
│     ├── 更新凭证                                             │
│     └── 加固系统                                             │
│     │                                                        │
│     ▼                                                        │
│  5. 恢复服务                                                 │
│     │                                                        │
│     ▼                                                        │
│  6. 事后分析                                                 │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 9.2 应急响应脚本

```bash
#!/bin/bash
# 应急响应脚本

# 1. 禁用所有外部访问
iptables -A INPUT -j DROP
iptables -A OUTPUT -j DROP

# 2. 撤销所有API密钥
redis-cli DEL "api_keys:*"

# 3. 备份日志
tar -czf /backup/incident_$(date +%Y%m%d_%H%M%S).tar.gz /var/log/synthia/

# 4. 通知管理员
curl -X POST "${SLACK_WEBHOOK}" \
  -d '{"text": "SECURITY INCIDENT: All access disabled"}'

# 5. 生成事件报告
./generate_incident_report.sh > /tmp/incident_report.txt
```

## 10. 相关文档

- [配置说明](../configuration/CONFIGURATION.md)
- [人机交互](human-in-the-loop.md)
- [错误恢复](error-recovery.md)

## 11. 参考资料

- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [Rust Security Guidelines](https://anssi-fr.github.io/rust-guide/)
- [API Security Best Practices](https://cheatsheetseries.owasp.org/cheatsheets/REST_Security_Cheat_Sheet.html)
