---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 快速开始

## 1. 概述

本指南帮助您在 5 分钟内启动 Synthia Server 并运行第一个 Agent。

## 2. 前置要求

- Rust 1.70+ 
- PostgreSQL 14+ (可选)
- Redis 6+ (可选)

## 3. 安装

### 1. 克隆仓库

```bash
git clone https://github.com/synthia/synthia.git
cd synthia
```

### 2. 构建项目

```bash
cargo build --release
```

### 3. 配置环境变量

```bash
export OPENAI_API_KEY=your-api-key
# 或
export ANTHROPIC_API_KEY=your-api-key
```

### 4. 启动服务

```bash
./target/release/synthia-server
```

服务将在 `http://localhost:8080` 启动。

## 4. 第一个请求

### 使用 cURL

```bash
curl -X POST http://localhost:8080/chat \
  -H "Content-Type: application/json" \
  -d '{
    "message": "你好，请介绍一下你自己"
  }'
```

### 使用 Python

```python
import requests

response = requests.post(
    "http://localhost:8080/chat",
    json={"message": "你好，请介绍一下你自己"}
)

print(response.json()["message"]["content"])
```

### 使用 TypeScript

```typescript
const response = await fetch('http://localhost:8080/chat', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ message: '你好，请介绍一下你自己' }),
});

const data = await response.json();
console.log(data.message.content);
```

## 5. 下一步

- [安装指南](installation.md) - 详细安装步骤
- [基本使用](basic-usage.md) - 基本功能介绍
- [API使用指南](../api-reference/API_GUIDE.md) - 完整API文档
