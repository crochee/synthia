# Memories 模块

记忆子系统，提供会话记忆的提取和整合功能。

## 核心组件

| 组件 | 可见性 | 功能描述 |
|------|--------|----------|
| `cron` | 公开 | 记忆任务调度 |
| `phase1` | pub(crate) | 阶段一：原始记忆提取 |
| `phase2` | pub(crate) | 阶段二：记忆整合 |
| `memory_root` | 公开 | 获取记忆根目录 |
| `store_stage1_output` | pub(crate) | 存储阶段一输出 |
| `store_consolidated_memory` | pub(crate) | 存储整合记忆 |

## 两阶段记忆管道

```
会话结束 → Phase1 提取 → Phase2 整合 → 存储记忆
```

### Phase 1: 原始记忆提取

从会话历史中提取原始记忆，包括关键决策、重要信息和用户偏好。

### Phase 2: 记忆整合

将原始记忆整合为有意义的摘要，进行主题分类和长期记忆存储。

## 存储结构

```
{workspace}/memories/
├── raw_memories.md           # 原始记忆
├── rollout_summaries/        # 会话摘要
│   └── {thread_id}.md
└── consolidated_{topic}.md   # 整合记忆
```

## 内部函数

- `call_model_intern` - 内部模型调用（使用 ModelRouter）
- `call_model_with_routed` - 使用已路由的提供者调用模型
