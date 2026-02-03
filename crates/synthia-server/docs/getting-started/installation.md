---
适用版本: synthia-server >= 0.1.0
最后更新: 2026-04-06
---

# 安装指南

## 1. 概述

本指南详细说明 Synthia Server 的安装步骤。

## 2. 系统要求

### 操作系统

- Linux (Ubuntu 20.04+, CentOS 8+, Debian 11+)
- macOS 11+
- Windows 10+ (WSL2)

### 软件依赖

| 依赖 | 版本 | 必需 | 说明 |
|------|------|------|------|
| Rust | 1.70+ | 是 | 编译和运行 |
| PostgreSQL | 14+ | 否 | 会话存储 |
| Redis | 6+ | 否 | 缓存 |

### 硬件要求

| 资源 | 最低 | 推荐 |
|------|------|------|
| CPU | 2核 | 4核+ |
| 内存 | 4GB | 8GB+ |
| 磁盘 | 10GB | 20GB+ |

## 3. 安装方式

### 方式一：从源码构建

#### 1. 安装 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

#### 2. 克隆仓库

```bash
git clone https://github.com/synthia/synthia.git
cd synthia
```

#### 3. 构建项目

```bash
# Debug 构建
cargo build

# Release 构建（推荐）
cargo build --release
```

#### 4. 安装二进制文件

```bash
cargo install --path crates/synthia-server
```

### 方式二：使用 Docker

#### 1. 拉取镜像

```bash
docker pull synthia/server:latest
```

#### 2. 运行容器

```bash
docker run -d \
  --name synthia-server \
  -p 8080:8080 \
  -e OPENAI_API_KEY=your-api-key \
  synthia/server:latest
```

#### 3. 使用 Docker Compose

```yaml
version: '3.8'

services:
  synthia-server:
    image: synthia/server:latest
    ports:
      - "8080:8080"
    environment:
      - OPENAI_API_KEY=${OPENAI_API_KEY}
    volumes:
      - ./data:/app/data
  
  postgres:
    image: postgres:14
    environment:
      - POSTGRES_DB=synthia
      - POSTGRES_USER=synthia
      - POSTGRES_PASSWORD=synthia
    volumes:
      - postgres-data:/var/lib/postgresql/data
  
  redis:
    image: redis:6
    volumes:
      - redis-data:/data

volumes:
  postgres-data:
  redis-data:
```

```bash
docker-compose up -d
```

### 方式三：下载预编译二进制

#### Linux

```bash
curl -L https://github.com/synthia/synthia/releases/latest/download/synthia-server-linux-x86_64.tar.gz | tar xz
chmod +x synthia-server
sudo mv synthia-server /usr/local/bin/
```

#### macOS

```bash
curl -L https://github.com/synthia/synthia/releases/latest/download/synthia-server-darwin-x86_64.tar.gz | tar xz
chmod +x synthia-server
sudo mv synthia-server /usr/local/bin/
```

## 4. 配置

### 1. 创建配置文件

```bash
mkdir -p ~/.synthia
cat > ~/.synthia/config.yaml << EOF
server:
  host: 0.0.0.0
  port: 8080

model:
  provider: openai
  model: gpt-4

database:
  url: postgresql://synthia:synthia@localhost:5432/synthia

redis:
  url: redis://localhost:6379
EOF
```

### 2. 设置环境变量

```bash
# API 密钥
export OPENAI_API_KEY=sk-xxx
# 或
export ANTHROPIC_API_KEY=sk-ant-xxx

# 配置文件路径（可选）
export SYNTHIA_CONFIG=~/.synthia/config.yaml
```

## 5. 启动服务

### 直接启动

```bash
synthia-server
```

### 使用 systemd (Linux)

```bash
# 创建服务文件
sudo cat > /etc/systemd/system/synthia.service << EOF
[Unit]
Description=Synthia Server
After=network.target

[Service]
Type=simple
User=synthia
WorkingDirectory=/home/synthia
Environment="OPENAI_API_KEY=sk-xxx"
ExecStart=/usr/local/bin/synthia-server
Restart=on-failure

[Install]
WantedBy=multi-user.target
EOF

# 启动服务
sudo systemctl daemon-reload
sudo systemctl enable synthia
sudo systemctl start synthia
```

### 验证安装

```bash
# 检查服务状态
curl http://localhost:8080/health

# 发送测试请求
curl -X POST http://localhost:8080/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello"}'
```

## 6. 升级

### 从源码升级

```bash
cd synthia
git pull origin main
cargo build --release
cargo install --path crates/synthia-server
```

### 使用 Docker 升级

```bash
docker pull synthia/server:latest
docker-compose up -d
```

## 7. 故障排查

### 端口被占用

```bash
# 检查端口
lsof -i :8080

# 修改配置使用其他端口
synthia-server --port 8081
```

### 数据库连接失败

```bash
# 检查数据库状态
systemctl status postgresql

# 测试连接
psql -h localhost -U synthia -d synthia
```

### API 密钥无效

```bash
# 验证密钥
curl https://api.openai.com/v1/models \
  -H "Authorization: Bearer $OPENAI_API_KEY"
```

## 8. 下一步

- [快速开始](quick-start.md) - 运行第一个请求
- [配置说明](../configuration/CONFIGURATION.md) - 详细配置选项
