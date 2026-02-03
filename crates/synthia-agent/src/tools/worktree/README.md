# Worktree Tools

工作树隔离工具模块，提供基于 Git Worktree 的目录隔离能力，支持并行任务执行。

## 工具列表

| 工具名称 | 功能描述 |
|----------|----------|
| `worktree_create` | 创建工作树 |
| `worktree_run` | 在工作树中执行命令 |
| `worktree_remove` | 删除工作树 |
| `worktree_events` | 查看工作树事件 |

## 交互顺序

```
Agent → 创建工作树 → 执行任务 → 清理工作树
```

## 在 Agent 中的作用

1. **目录隔离**: 不同任务在不同工作树中执行
2. **并行执行**: 支持多个工作树同时运行
3. **Git 集成**: 基于 Git Worktree 实现
4. **任务绑定**: 可将任务绑定到特定工作树

## 工作树结构

```
.worktrees/
├── index.json      # 工作树索引
└── events.jsonl    # 事件日志
```

## 使用示例

### 创建工作树

```json
{
  "name": "worktree_create",
  "arguments": {
    "name": "feature-branch",
    "baseRef": "main"
  }
}
```

### 在工作树中执行

```json
{
  "name": "worktree_run",
  "arguments": {
    "name": "feature-branch",
    "command": "npm run build"
  }
}
```

## 设计理念

> 隔离创造可能

Worktree 系统解决的问题：
1. **环境隔离**: 避免任务间相互影响
2. **并行处理**: 多任务同时执行
3. **版本控制**: 利用 Git 管理分支
4. **资源清理**: 自动清理工作树
