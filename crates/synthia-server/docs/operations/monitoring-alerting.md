---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 监控告警

## 1. 概述

本文档说明 Synthia Agent 的监控和告警系统，包括指标收集、日志管理、告警配置和仪表板。

## 2. 监控架构

```
┌─────────────────────────────────────────────────────────────┐
│                    Monitoring Architecture                   │
│                                                              │
│  ┌──────────────┐                                            │
│  │   Synthia    │                                            │
│  │   Server     │                                            │
│  └──────┬───────┘                                            │
│         │                                                    │
│         ├────── Metrics ──────▶ Prometheus                   │
│         │                         │                          │
│         │                         ▼                          │
│         │                      Grafana                       │
│         │                                                    │
│         ├────── Logs ────────▶ ELK Stack / Loki             │
│         │                                                    │
│         ├────── Traces ──────▶ Jaeger                        │
│         │                                                    │
│         └────── Alerts ──────▶ Alertmanager                  │
│                                    │                         │
│                                    ▼                         │
│                              Slack/Email/PagerDuty           │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## 3. 指标收集

### 3.1 系统指标

```rust
use prometheus::{Counter, Gauge, Histogram, Registry};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();

    // 请求计数
    pub static ref HTTP_REQUESTS_TOTAL: Counter = register_counter_with_opts!(
        CounterOpts {
            name: "http_requests_total",
            help: "Total number of HTTP requests",
            const_labels: HashMap::new(),
            variable_labels: vec!["method".to_string(), "path".to_string(), "status".to_string()],
        }
    ).unwrap();

    // 请求延迟
    pub static ref HTTP_REQUEST_DURATION: Histogram = register_histogram_with_opts!(
        HistogramOpts {
            name: "http_request_duration_seconds",
            help: "HTTP request duration in seconds",
            const_labels: HashMap::new(),
            variable_labels: vec!["method".to_string(), "path".to_string()],
            buckets: vec![0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0],
        }
    ).unwrap();

    // 活跃会话
    pub static ref ACTIVE_SESSIONS: Gauge = register_gauge_with_opts!(
        GaugeOpts {
            name: "active_sessions",
            help: "Number of active sessions",
            const_labels: HashMap::new(),
            variable_labels: vec![],
        }
    ).unwrap();

    // Agent 指标
    pub static ref AGENT_TOOL_CALLS_TOTAL: Counter = register_counter_with_opts!(
        CounterOpts {
            name: "agent_tool_calls_total",
            help: "Total number of tool calls",
            const_labels: HashMap::new(),
            variable_labels: vec!["agent_name".to_string(), "tool_name".to_string(), "status".to_string()],
        }
    ).unwrap();

    pub static ref AGENT_EXECUTION_DURATION: Histogram = register_histogram_with_opts!(
        HistogramOpts {
            name: "agent_execution_duration_seconds",
            help: "Agent execution duration in seconds",
            const_labels: HashMap::new(),
            variable_labels: vec!["agent_name".to_string()],
            buckets: vec![0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0, 120.0],
        }
    ).unwrap();

    pub static ref AGENT_CONTEXT_TOKENS: Gauge = register_gauge_with_opts!(
        GaugeOpts {
            name: "agent_context_tokens",
            help: "Current context token count",
            const_labels: HashMap::new(),
            variable_labels: vec!["session_id".to_string()],
        }
    ).unwrap();

    // 缓存指标
    pub static ref CACHE_OPERATIONS_TOTAL: Counter = register_counter_with_opts!(
        CounterOpts {
            name: "cache_operations_total",
            help: "Total number of cache operations",
            const_labels: HashMap::new(),
            variable_labels: vec!["cache_name".to_string(), "operation".to_string(), "status".to_string()],
        }
    ).unwrap();
}
```

### 3.2 业务指标

```rust
pub struct BusinessMetrics {
    // 用户指标
    pub users_total: Counter,
    pub users_active: Gauge,

    // 会话指标
    pub sessions_created_total: Counter,
    pub sessions_completed_total: Counter,
    pub sessions_failed_total: Counter,

    // 工具指标
    pub tools_available: Gauge,
    pub tools_execution_success_rate: Gauge,

    // 成本指标
    pub tokens_used_total: Counter,
    pub api_cost_total: Counter,
}

impl BusinessMetrics {
    pub fn record_session_created(&self) {
        self.sessions_created_total.inc();
    }

    pub fn record_session_completed(&self) {
        self.sessions_completed_total.inc();
    }

    pub fn record_tokens_used(&self, tokens: u64, model: &str) {
        self.tokens_used_total
            .with_label_values(&[model])
            .inc_by(tokens);
    }
}
```

### 3.3 自定义指标

```rust
pub fn record_custom_metric(name: &str, value: f64, labels: HashMap<String, String>) {
    let gauge = Gauge::new(name, format!("Custom metric: {}", name)).unwrap();
    
    for (key, value) in labels {
        gauge.with_label_values(&[&value]);
    }
    
    gauge.set(value);
}
```

## 4. 日志管理

### 4.1 结构化日志

```rust
use tracing::{info, warn, error, instrument};
use tracing_subscriber::fmt::format::FmtContext;

#[instrument(skip(agent))]
pub async fn execute_agent(agent: &Agent, request: Request) -> Result<Response> {
    info!(
        agent_name = %agent.name,
        request_id = %request.id,
        "Starting agent execution"
    );

    match agent.execute(request).await {
        Ok(response) => {
            info!(
                agent_name = %agent.name,
                request_id = %request.id,
                duration_ms = response.duration.as_millis(),
                "Agent execution completed"
            );
            Ok(response)
        }
        Err(e) => {
            error!(
                agent_name = %agent.name,
                request_id = %request.id,
                error = %e,
                "Agent execution failed"
            );
            Err(e)
        }
    }
}
```

### 4.2 日志配置

```yaml
logging:
  level: info
  format: json
  output:
    - type: file
      path: /var/log/synthia/app.log
      rotation: daily
      max_size: 100MB
      max_files: 30
    
    - type: stdout
      format: pretty
  
  fields:
    - timestamp
    - level
    - target
    - message
    - request_id
    - session_id
    - agent_name
  
  filters:
    - level: warn
      target: hyper
    - level: error
      target: sqlx
```

### 4.3 日志聚合

```yaml
logging:
  aggregation:
    enabled: true
    type: elasticsearch
    
    elasticsearch:
      url: http://localhost:9200
      index: synthia-logs
      batch_size: 100
      flush_interval: 5s
```

## 5. 分布式追踪

### 5.1 OpenTelemetry 配置

```rust
use opentelemetry::{
    global,
    sdk::trace::TracerProvider,
    trace::TracerProvider as _,
};
use tracing_subscriber::layer::SubscriberExt;

pub fn init_tracing(service_name: &str) -> Result<()> {
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(opentelemetry_otlp::new_exporter().tonic())
        .with_trace_config(
            opentelemetry::sdk::trace::Config::default()
                .with_resource(opentelemetry::sdk::Resource::new(vec![
                    opentelemetry::KeyValue::new("service.name", service_name),
                ]))
        )
        .install_batch(opentelemetry::runtime::Tokio)?;

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(telemetry)
        .try_init()?;

    Ok(())
}
```

### 5.2 追踪注解

```rust
use tracing::{info_span, instrument};

#[instrument(skip(agent))]
pub async fn process_request(agent: &Agent, request: Request) -> Result<Response> {
    let span = info_span!("process_request", request_id = %request.id);
    let _enter = span.enter();

    // 子操作
    let context = prepare_context(&request).await?;
    let result = agent.execute(context).await?;

    Ok(result)
}

#[instrument(skip(request))]
async fn prepare_context(request: &Request) -> Result<Context> {
    // ...
}
```

## 6. 告警配置

### 6.1 告警规则

```yaml
alerting:
  rules:
    - name: high_error_rate
      expr: rate(http_requests_total{status=~"5.."}[5m]) / rate(http_requests_total[5m]) > 0.05
      for: 5m
      severity: critical
      annotations:
        summary: "High error rate detected"
        description: "Error rate is {{ $value | humanizePercentage }}"
    
    - name: high_latency
      expr: histogram_quantile(0.99, rate(http_request_duration_seconds_bucket[5m])) > 2
      for: 5m
      severity: warning
      annotations:
        summary: "High latency detected"
        description: "P99 latency is {{ $value }}s"
    
    - name: agent_execution_timeout
      expr: rate(agent_execution_duration_seconds_bucket{le="120"}[5m]) / rate(agent_execution_duration_seconds_count[5m]) < 0.95
      for: 10m
      severity: warning
      annotations:
        summary: "Agent execution timeout"
        description: "More than 5% of agent executions are timing out"
    
    - name: cache_hit_rate_low
      expr: rate(cache_operations_total{operation="hit"}[5m]) / rate(cache_operations_total[5m]) < 0.8
      for: 10m
      severity: warning
      annotations:
        summary: "Low cache hit rate"
        description: "Cache hit rate is {{ $value | humanizePercentage }}"
    
    - name: memory_usage_high
      expr: (node_memory_MemTotal_bytes - node_memory_MemAvailable_bytes) / node_memory_MemTotal_bytes > 0.9
      for: 5m
      severity: critical
      annotations:
        summary: "High memory usage"
        description: "Memory usage is {{ $value | humanizePercentage }}"
```

### 6.2 告警路由

```yaml
alerting:
  routes:
    - receiver: 'slack-critical'
      match:
        severity: critical
      continue: false
    
    - receiver: 'slack-warning'
      match:
        severity: warning
      continue: false
    
    - receiver: 'email-all'
      match_re:
        severity: .*
      continue: false

  receivers:
    - name: 'slack-critical'
      slack_configs:
        - api_url: "${SLACK_WEBHOOK_URL}"
          channel: '#synthia-critical'
          title: '🚨 {{ .Status | toUpper }}: {{ .CommonAnnotations.summary }}'
          text: '{{ .CommonAnnotations.description }}'
    
    - name: 'slack-warning'
      slack_configs:
        - api_url: "${SLACK_WEBHOOK_URL}"
          channel: '#synthia-alerts'
          title: '⚠️ {{ .Status | toUpper }}: {{ .CommonAnnotations.summary }}'
          text: '{{ .CommonAnnotations.description }}'
    
    - name: 'email-all'
      email_configs:
        - to: 'team@example.com'
          from: 'alerts@synthia.com'
          smarthost: 'smtp.example.com:587'
          auth_username: '${SMTP_USER}'
          auth_password: '${SMTP_PASSWORD}'
```

## 7. Grafana 仪表板

### 7.1 系统仪表板

```json
{
  "dashboard": {
    "title": "Synthia System Dashboard",
    "panels": [
      {
        "title": "Request Rate",
        "type": "graph",
        "targets": [
          {
            "expr": "rate(http_requests_total[5m])",
            "legendFormat": "{{method}} {{path}}"
          }
        ]
      },
      {
        "title": "Request Latency",
        "type": "heatmap",
        "targets": [
          {
            "expr": "rate(http_request_duration_seconds_bucket[5m])",
            "legendFormat": "{{method}} {{path}}"
          }
        ]
      },
      {
        "title": "Error Rate",
        "type": "gauge",
        "targets": [
          {
            "expr": "rate(http_requests_total{status=~\"5..\"}[5m]) / rate(http_requests_total[5m])",
            "legendFormat": "Error Rate"
          }
        ]
      },
      {
        "title": "Active Sessions",
        "type": "stat",
        "targets": [
          {
            "expr": "active_sessions",
            "legendFormat": "Sessions"
          }
        ]
      }
    ]
  }
}
```

### 7.2 Agent 仪表板

```json
{
  "dashboard": {
    "title": "Synthia Agent Dashboard",
    "panels": [
      {
        "title": "Tool Calls",
        "type": "graph",
        "targets": [
          {
            "expr": "rate(agent_tool_calls_total[5m])",
            "legendFormat": "{{agent_name}} - {{tool_name}}"
          }
        ]
      },
      {
        "title": "Execution Duration",
        "type": "heatmap",
        "targets": [
          {
            "expr": "rate(agent_execution_duration_seconds_bucket[5m])",
            "legendFormat": "{{agent_name}}"
          }
        ]
      },
      {
        "title": "Context Tokens",
        "type": "graph",
        "targets": [
          {
            "expr": "agent_context_tokens",
            "legendFormat": "{{session_id}}"
          }
        ]
      },
      {
        "title": "Cache Hit Rate",
        "type": "gauge",
        "targets": [
          {
            "expr": "rate(cache_operations_total{operation=\"hit\"}[5m]) / rate(cache_operations_total[5m])",
            "legendFormat": "Hit Rate"
          }
        ]
      }
    ]
  }
}
```

## 8. 最佳实践

### 8.1 监控原则

1. **监控一切**：所有关键指标都应该被监控
2. **告警有意义**：避免告警疲劳
3. **快速响应**：建立快速响应机制
4. **持续改进**：根据反馈优化监控

### 8.2 告警原则

1. **可操作**：每个告警都应该有明确的处理步骤
2. **避免噪音**：减少误报和重复告警
3. **分级处理**：根据严重性分级处理
4. **及时通知**：选择合适的通知渠道

### 8.3 日志原则

1. **结构化**：使用结构化日志格式
2. **上下文丰富**：包含足够的上下文信息
3. **敏感信息**：避免记录敏感信息
4. **合理级别**：使用合适的日志级别

## 9. 相关文档

- [性能优化](performance-optimization.md)
- [故障排查](troubleshooting.md)
- [第三方服务集成](../integration/third-party-services.md)

## 10. 参考资料

- [Prometheus Best Practices](https://prometheus.io/docs/practices/)
- [Grafana Dashboard Best Practices](https://grafana.com/docs/grafana/latest/best-practices/)
- [OpenTelemetry Documentation](https://opentelemetry.io/docs/)
