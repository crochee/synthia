# File System (FS) Tools

文件系统工具模块，提供文件操作能力，是 Agent 与本地文件系统交互的基础工具集。

## 工具列表

| 工具名称 | 功能描述 |
|----------|----------|
| `read` | 读取文件内容 |
| `write` | 写入内容到文件 |
| `create_directory` | 创建目录 |
| `delete` | 删除文件或目录 |
| `directory_tree` | 获取目录树结构 |
| `edit` | 编辑文件（替换文本） |
| `glob` | 模式匹配文件 |
| `grep` | 文本搜索 |
| `list_directory` | 列出目录内容 |
| `move_file` | 移动/重命名文件 |

## 交互顺序

```
Agent决策 → FS工具调用 → 文件系统操作 → 结果返回 → Agent继续推理
```

## 在 Agent 中的作用

1. **代码读取**: Agent 通过 `read` 工具理解现有代码结构
2. **代码修改**: Agent 使用 `edit` 工具进行代码修改
3. **项目探索**: 使用 `directory_tree` 和 `list_directory` 了解项目结构
4. **代码搜索**: 使用 `grep` 和 `glob` 定位特定代码位置
5. **文件创建**: 使用 `write` 和 `create_directory` 创建新文件

## Agent 运行机制

### 典型工作流

```
1. 用户请求 → "修复某bug"
      ↓
2. Agent 读取相关文件 (read)
      ↓
3. 分析代码结构 (directory_tree)
      ↓
4. 搜索相关代码 (grep)
      ↓
5. 修改代码 (edit/write)
      ↓
6. 验证修改结果 (read)
```

### 安全机制

- 所有路径操作都经过工作目录验证
- 禁止访问工作目录之外的文件
- 删除操作有安全确认

## 使用示例

### 读取文件

```
Tool: read
Args: { "path": "/workspace/src/main.rs" }
```

### 写入文件

```
Tool: write
Args: { "path": "/workspace/src/main.rs", "content": "..." }
```

### 编辑文件

```
Tool: edit
Args: { "path": "/workspace/src/main.rs", "old_str": "old code", "new_str": "new code" }
```

### 搜索代码

```
Tool: grep
Args: { "pattern": "function name", "path": "/workspace/src" }
```

### 目录树

```
Tool: directory_tree
Args: { "path": "/workspace" }
```
