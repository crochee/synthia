# Synthia-Agent 优化计划

## 1. 当前实现分析

### 1.1 优点
- 结构简单清晰
- 支持基本的模型调用和工具执行
- 支持流式响应

### 1.2 缺点
1. **工具调用处理不完善**：
   - 工具执行后没有继续对话（只是执行了工具，但没有将结果返回给模型继续处理）
   - 没有处理工具执行的错误情况
   - 没有工具审批机制

2. **对话上下文管理**：
   - 没有对话历史压缩机制
   - 没有会话管理

3. **错误处理**：
   - 错误处理比较简单
   - 没有详细的错误分类

4. **事件系统**：
   - 只返回 Message 流，没有更丰富的事件类型

5. **缺少高级功能**：
   - 没有模型切换机制
   - 没有会话命名
   - 没有命令处理
   - 没有 ActionRequired 消息处理

## 2. 优化方案

### 2.1 引入 AgentEvent 枚举

**目的**：提供更丰富的事件类型，支持更多场景

**实现**：
```rust
#[derive(Clone, Debug)]
pub enum AgentEvent {
    Message(Message),
    McpNotification((String, ServerNotification)),
    ModelChange { model: String, mode: String },
    HistoryReplaced(Vec<Message>),
}
```

### 2.2 抽象用户输入处理接口

**目的**：为用户输入处理提供统一接口，便于后续实现不同的输入方式（如bash或其他）

**实现**：
```rust
#[async_trait]
pub trait UserInputHandler: Send + Sync {
    async fn handle_input(&self, input: String) -> Result<String>;
    async fn get_input(&self, prompt: &str) -> Result<String>;
}

// 默认实现（直接返回输入）
pub struct DefaultUserInputHandler;

#[async_trait]
impl UserInputHandler for DefaultUserInputHandler {
    async fn handle_input(&self, input: String) -> Result<String> {
        Ok(input)
    }
    
    async fn get_input(&self, prompt: &str) -> Result<String> {
        // 这里可以后续实现具体的输入获取方式
        Ok(format!("{prompt} (default input)"))
    }
}
```

### 2.3 完善工具调用处理

**目的**：实现完整的工具调用流程，包括工具执行后的对话继续

**实现**：
1. 修改 `process_tool_calls` 方法，使其返回处理后的对话
2. 在 `process_conversation` 中，当工具执行完成后，将结果添加到对话历史并继续调用模型
3. 实现工具审批机制

### 2.4 抽象上下文管理接口

**目的**：为对话历史管理提供统一接口，便于后续实现

**实现**：
```rust
#[async_trait]
pub trait ContextManager: Send + Sync {
    async fn check_compaction_needed(&self, conversation: &[Message]) -> Result<bool>;
    async fn compact(&self, conversation: &[Message]) -> Result<Vec<Message>>;
}

// 默认实现（无压缩）
pub struct DefaultContextManager;

#[async_trait]
impl ContextManager for DefaultContextManager {
    async fn check_compaction_needed(&self, _conversation: &[Message]) -> Result<bool> {
        Ok(false)
    }
    
    async fn compact(&self, conversation: &[Message]) -> Result<Vec<Message>> {
        Ok(conversation.to_vec())
    }
}
```

### 2.5 抽象会话管理接口

**目的**：为会话管理提供统一接口，便于后续实现

**实现**：
```rust
#[async_trait]
pub trait SessionManager: Send + Sync {
    async fn get_session(&self, session_id: &str) -> Result<Option<Session>>;
    async fn create_session(&self) -> Result<Session>;
    async fn update_session(&self, session: &Session) -> Result<()>;
    async fn delete_session(&self, session_id: &str) -> Result<()>;
    async fn add_message(&self, session_id: &str, message: &Message) -> Result<()>;
    async fn get_conversation(&self, session_id: &str) -> Result<Vec<Message>>;
    async fn replace_conversation(&self, session_id: &str, conversation: &[Message]) -> Result<()>;
}

// 默认实现（内存会话管理）
pub struct InMemorySessionManager {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}

impl InMemorySessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

// 实现 SessionManager trait
#[async_trait]
impl SessionManager for InMemorySessionManager {
    // 实现各个方法...
}
```

### 2.6 完善错误处理

**目的**：提供更详细的错误处理和分类

**实现**：
1. 扩展 `AgentError` 枚举，增加更多错误类型
2. 实现错误处理和转换方法
3. 在各个方法中添加详细的错误处理

### 2.7 增加命令处理功能

**目的**：支持用户输入命令的处理

**实现**：
1. 实现 `execute_command` 方法，处理用户输入的命令
2. 在 `reply` 方法中，检查并执行用户输入的命令
3. 实现常见命令（如 /clear、/compact 等）

### 2.8 增加模型切换机制

**目的**：支持不同模型之间的切换

**实现**：
1. 扩展 `AgentConfig`，增加模型配置选项
2. 实现模型切换逻辑
3. 在 `process_conversation` 中，根据需要切换模型

### 2.9 增加会话命名功能

**目的**：支持自动为会话生成名称

**实现**：
1. 实现 `maybe_update_name` 方法，为会话生成名称
2. 在会话创建或更新时，自动生成会话名称

## 3. 实现步骤

### 3.1 步骤 1：引入 AgentEvent 枚举
- 创建 `types.rs` 文件，定义 `AgentEvent` 枚举
- 修改 `reply` 方法的返回类型，从 `BoxStream<'_, Result<Message>>` 改为 `BoxStream<'_, Result<AgentEvent>>`

### 3.2 步骤 2：抽象用户输入处理接口
- 创建 `user_input.rs` 文件，定义 `UserInputHandler` trait
- 实现默认的 `DefaultUserInputHandler`
- 在 `Agent` 结构体中，添加 `user_input_handler` 字段

### 3.3 步骤 3：完善工具调用处理
- 修改 `process_tool_calls` 方法，使其返回处理后的对话
- 在 `process_conversation` 中，添加工具执行后的对话继续逻辑
- 实现工具审批机制

### 3.4 步骤 4：抽象上下文管理接口
- 创建 `context_manager.rs` 文件，定义 `ContextManager` trait
- 实现默认的 `DefaultContextManager`
- 在 `Agent` 结构体中，添加 `context_manager` 字段

### 3.5 步骤 5：抽象会话管理接口
- 创建 `session_manager.rs` 文件，定义 `SessionManager` trait
- 实现默认的 `InMemorySessionManager`
- 在 `Agent` 结构体中，添加 `session_manager` 字段

### 3.6 步骤 6：完善错误处理
- 扩展 `error.rs` 文件，增加更多错误类型
- 在各个方法中添加详细的错误处理

### 3.7 步骤 7：增加命令处理功能
- 实现 `execute_command` 方法
- 在 `reply` 方法中，添加命令检查和执行逻辑
- 实现常见命令

### 3.8 步骤 8：增加模型切换机制
- 扩展 `AgentConfig`，增加模型配置选项
- 实现模型切换逻辑

### 3.9 步骤 9：增加会话命名功能
- 实现 `maybe_update_name` 方法
- 在会话创建或更新时，添加会话命名逻辑

## 4. 预期结果

通过以上优化，synthia-agent 将具备以下功能：

1. **更丰富的事件系统**：支持 Message、McpNotification、ModelChange、HistoryReplaced 等事件类型
2. **抽象的用户输入处理**：提供统一的用户输入接口，便于后续实现不同的输入方式
3. **完整的工具调用流程**：工具执行后会将结果返回给模型继续处理
4. **抽象的上下文管理**：提供统一接口，便于后续实现具体的压缩策略
5. **抽象的会话管理**：提供统一接口，便于后续实现不同的会话存储方式
6. **详细的错误处理**：提供更详细的错误类型和处理
7. **支持命令处理**：支持用户输入命令的处理
8. **支持模型切换**：可以根据需要切换不同的模型
9. **支持会话命名**：自动为会话生成名称

这些功能将使 synthia-agent 更加完善和强大，提供更好的用户体验，同时保持扩展性，便于后续添加更多功能。