# synthia-agent 循环迭代退出逻辑优化计划

## 一、核心概念：ReAct vs Ralph Loop

根据图片对比：

### 1.1 ReAct 模式

```
┌─────────────────────────────────────────────────────────────────┐
│                        ReAct Loop                                  │
│                                                                  │
│    ┌─────────────────────────────────────────────────────────┐   │
│    │              Tool Use Loop (LLM 内部循环)                 │   │
│    │     ┌──────────────┐         ┌──────────────┐           │   │
│    │     │    LLM      │────────▶│    Tools    │           │   │
│    │     │  (思考/推理)  │         │   (执行工具)  │           │   │
│    │     └──────┬───────┘         └──────┬───────┘           │   │
│    │            │ tool_results          │                   │   │
│    │            ◀───────────────────────────┘                   │   │
│    └─────────────────────────────────────────────────────────┘   │
│                              │                                      │
│                              ▼                                      │
│    ┌─────────────────────────────────────────────────────────┐   │
│    │              Messages / Conversation History               │   │
│    │              (状态在内部累积)                               │   │
│    └─────────────────────────────────────────────────────────┘   │
│                                                                  │
│    退出条件：LLM 决定不调用工具时                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 1.2 Ralph Loop 模式

```
┌─────────────────────────────────────────────────────────────────┐
│                     Ralph Loop (外部循环)                           │
│                                                                  │
│    ┌─────────────────────────────────────────────────────────┐   │
│    │              External Loop (外部 for 循环)                │   │
│    │     ┌──────────────────────────────────────────────┐    │   │
│    │     │           单次 LLM 调用                        │    │   │
│    │     │  ┌──────────┐    ┌──────────┐    ┌────────┐  │    │   │
│    │     │  │    LLM   │───▶│  Action  │───▶│ Output │  │    │   │
│    │     │  └──────────┘    └──────────┘    └────────┘  │    │   │
│    │     └──────────────────────────────────────────────┘    │   │
│    │                              │                              │   │
│    │                              ▼                              │   │
│    │     ┌──────────────────────────────────────────────┐    │   │
│    │     │  检查输出是否包含 "completion criteria"       │    │   │
│    │     └──────────────────────────────────────────────┘    │   │
│    └─────────────────────────────────────────────────────────┘   │
│                              │                                      │
│                              ▼                                      │
│    ┌─────────────────────────────────────────────────────────┐   │
│    │              External Files (Task, Progress)               │   │
│    │              (状态通过外部文件管理)                        │   │
│    └─────────────────────────────────────────────────────────┘   │
│                                                                  │
│    退出条件：检查输出是否包含完成标志                                │
└─────────────────────────────────────────────────────────────────┘
```

---

## 二、优化方案：混合模式 + 智能退化

### 2.1 设计目标

1. **混合模式为主**：外层 Ralph Loop + 内层 ReAct Loop
2. **智能退化**：任务简单时自动退化为纯 ReAct 模式
3. **自动检测**：基于任务特征和运行时的表现自动选择最优模式

### 2.2 架构设计

```
┌─────────────────────────────────────────────────────────────────────┐
│                    HybridLoop (混合模式 + 智能退化)                         │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                    TaskAnalyzer                                  │ │
│  │  ┌──────────────────────────────────────────────────────────┐  │ │
│  │  │  analyze_task(task) -> TaskComplexity                    │  │ │
│  │  │                                                           │  │ │
│  │  │  评估因素:                                               │  │ │
│  │  │  - 任务描述长度                                           │  │ │
│  │  │  - 是否包含明确完成条件                                     │  │ │
│  │  │  - 是否包含测试/验证关键词                                   │  │ │
│  │  │  - 是否需要多步骤完成                                        │  │ │
│  │  │  - 历史运行数据（如果有）                                    │  │ │
│  │  └──────────────────────────────────────────────────────────┘  │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                              │                                        │
│                              ▼                                        │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                    LoopController                                │ │
│  │                                                                 │ │
│  │   analyze_task() ──▶ TaskComplexity                              │ │
│  │        │                    │                                    │ │
│  │        │                    ▼                                    │ │
│  │        │              ┌─────────────┐                             │ │
│  │        │              │  Simple?   │                             │ │
│  │        │              └─────┬─────┘                             │ │
│  │        │                    │                                    │ │
│  │        │           ┌────────┴────────┐                          │ │
│  │        │           │                 │                          │ │
│  │        │           ▼                 ▼                          │ │
│  │        │    ┌──────────────┐  ┌─────────────────┐            │ │
│  │        │    │退化为ReAct  │  │  Hybrid模式      │            │ │
│  │        │    │  模式       │  │ (Ralph外层+     │            │ │
│  │        │    │             │  │  ReAct内层)     │            │ │
│  │        │    └──────────────┘  └─────────────────┘            │ │
│  └────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.3 智能退化机制

```
┌─────────────────────────────────────────────────────────────────────┐
│                      智能退化决策树                                        │
│                                                                      │
│                                                                      │
│                        ┌──────────────┐                              │
│                        │  接收任务    │                              │
│                        └──────┬───────┘                              │
│                               │                                       │
│                               ▼                                       │
│                        ┌──────────────┐                              │
│                        │TaskAnalyzer  │                              │
│                        │  分析任务    │                              │
│                        └──────┬───────┘                              │
│                               │                                       │
│                               ▼                                       │
│                      ┌────────────────┐                             │
│                      │ is_simple_task │                             │
│                      └───────┬────────┘                             │
│                              │                                       │
│              ┌───────────────┴───────────────┐                      │
│              │                               │                      │
│              ▼                               ▼                      │
│     ┌────────────────┐           ┌─────────────────────────┐        │
│     │   YES         │           │         NO             │        │
│     │ 退化为 ReAct  │           │ 使用 Hybrid 模式        │        │
│     │   模式        │           │ (Ralph 外层+ReAct内层)│        │
│     └────────────────┘           └─────────────────────────┘        │
│                                                                      │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                     运行时退化触发条件                          │   │
│  │                                                              │   │
│  │  • 第一次 ReAct 迭代就产生了有效输出（无工具调用）             │   │
│  │  • 任务在 1-2 个 ReAct 迭代内完成                              │   │
│  │  • 用户没有提供外部验证条件                                      │   │
│  │  • max_ralph_iterations = 1                                   │   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 三、核心类型定义

### 3.1 任务复杂度分析

```rust
// crates/synthia-agent/src/agent/loop/analyzer.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskComplexity {
    /// 简单任务：单个步骤即可完成
    Simple,
    /// 中等任务：需要多步骤，但可预测
    Medium,
    /// 复杂任务：需要多次迭代和验证
    Complex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAnalysis {
    pub complexity: TaskComplexity,
    pub has_explicit_criteria: bool,
    pub estimated_iterations: u32,
    pub requires_verification: bool,
    pub keywords_detected: Vec<String>,
}

pub struct TaskAnalyzer;

impl TaskAnalyzer {
    pub fn analyze(task: &str) -> TaskAnalysis {
        let task_lower = task.to_lowercase();
        let word_count = task.split_whitespace().count();
        
        // 检测完成条件关键词
        let criteria_keywords = [
            "test", "verify", "check", "ensure", "confirm",
            "build", "run", "deploy", "create", "implement",
            "refactor", "fix", "migrate",
        ];
        
        let detected: Vec<String> = criteria_keywords
            .iter()
            .filter(|k| task_lower.contains(*k))
            .map(|k| k.to_string())
            .collect();
        
        // 判断是否有明确的完成条件
        let has_explicit_criteria = detected.len() >= 2 
            || task_lower.contains("successfully")
            || task_lower.contains("complete");
        
        // 估算复杂度
        let complexity = if word_count < 20 && detected.len() <= 1 {
            TaskComplexity::Simple
        } else if word_count < 50 && detected.len() < 3 {
            TaskComplexity::Medium
        } else {
            TaskComplexity::Complex
        };
        
        let requires_verification = detected.iter().any(|k| 
            matches!(k.as_str(), "test" | "verify" | "check" | "ensure")
        );
        
        TaskAnalysis {
            complexity,
            has_explicit_criteria,
            estimated_iterations: match complexity {
                TaskComplexity::Simple => 1,
                TaskComplexity::Medium => 3,
                TaskComplexity::Complex => 5,
            },
            requires_verification,
            keywords_detected: detected,
        }
    }
    
    pub fn should_use_react(analysis: &TaskAnalysis) -> bool {
        matches!(analysis.complexity, TaskComplexity::Simple) 
            && !analysis.requires_verification
    }
}
```

### 3.2 循环配置

```rust
// crates/synthia-agent/src/agent/loop/config.rs

#[derive(Debug, Clone)]
pub struct LoopConfig {
    pub mode: LoopMode,
    pub max_steps: u32,
    pub enable_detection: bool,
    pub auto_downgrade: bool,        // 自动退化开关
    pub downgrade_threshold: u32,      // 退化阈值
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LoopMode {
    /// 纯 ReAct 模式
    ReAct,
    /// 混合模式（Ralph 外层 + ReAct 内层）
    Hybrid(HybridConfig),
    /// Ralph 模式（纯外部循环）
    Ralph(RalphConfig),
}

#[derive(Debug, Clone)]
pub struct HybridConfig {
    pub ralph_max_iterations: u32,
    pub react_max_steps: u32,
    pub state_dir: PathBuf,
    pub completion_markers: Vec<String>,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            mode: LoopMode::Hybrid(HybridConfig::default()),
            max_steps: 50,
            enable_detection: true,
            auto_downgrade: true,      // 默认开启自动退化
            downgrade_threshold: 3,    // 3 次迭代内完成则退化
        }
    }
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            ralph_max_iterations: 5,
            react_max_steps: 20,
            state_dir: PathBuf::from(".synthia/ralph"),
            completion_markers: vec![
                "done".to_string(),
                "completed".to_string(),
                "finished".to_string(),
                "success".to_string(),
            ],
        }
    }
}
```

### 3.3 退出原因

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    // ReAct 级别
    NormalComplete,
    Cancelled,
    MaxStepsReached,
    
    // Ralph/Hybrid 级别
    RalphLoopShipped,
    RalphLoopMaxIterations,
    RalphLoopBlocked,
    
    // 自动退化
    DowngradedToReAct,
    
    // 错误
    Error(String),
}
```

---

## 四、详细实现步骤

### 4.1 第一阶段：任务分析器

#### 步骤 1：创建 TaskAnalyzer

```rust
// crates/synthia-agent/src/agent/loop/analyzer.rs

pub struct TaskAnalyzer;

impl TaskAnalyzer {
    pub fn analyze(task: &str) -> TaskAnalysis;
    pub fn should_use_react(analysis: &TaskAnalysis) -> bool;
    pub fn should_downgrade_during_runtime(&self, iterations: u32, has_progress: bool) -> bool;
}
```

### 4.2 第二阶段：混合循环控制器

#### 步骤 2：实现 HybridLoop

```rust
// crates/synthia-agent/src/agent/loop/hybrid.rs

pub struct HybridLoop {
    config: LoopConfig,
    state: LoopState,
    analyzer: TaskAnalyzer,
    is_downgraded: bool,
}

impl HybridLoop {
    pub fn new(config: LoopConfig, task: &str) -> Self {
        // 任务分析
        let analysis = TaskAnalyzer::analyze(task);
        
        // 决定初始模式
        let mode = if config.auto_downgrade && TaskAnalyzer::should_use_react(&analysis) {
            tracing::info!("Task detected as simple, using ReAct mode");
            LoopMode::ReAct
        } else {
            config.mode.clone()
        };
        
        Self {
            config: LoopConfig { mode, ..config },
            state: LoopState::new(),
            analyzer: TaskAnalyzer,
            is_downgraded: false,
        }
    }
    
    pub async fn run(&mut self, task: &str) -> Result<ExitReason, AgentError> {
        match &self.config.mode {
            LoopMode::ReAct => {
                self.run_react(task).await
            }
            LoopMode::Hybrid(_) => {
                self.run_hybrid(task).await
            }
            LoopMode::Ralph(_) => {
                self.run_ralph(task).await
            }
        }
    }
    
    async fn run_hybrid(&mut self, task: &str) -> Result<ExitReason, AgentError> {
        let hybrid_config = match &self.config.mode {
            LoopMode::Hybrid(c) => c,
            _ => unreachable!(),
        };
        
        for ralph_iter in 1..=hybrid_config.ralph_max_iterations {
            self.state.ralph_iteration = ralph_iter;
            
            // ReAct 内层循环
            let react_exit = self.run_react_iteration(task, ralph_iter).await?;
            
            // 检查是否应该退化
            if self.should_downgrade(&react_exit, ralph_iter) {
                tracing::info!("Downgrading to ReAct mode after {} iterations", ralph_iter);
                self.is_downgraded = true;
                return Ok(ExitReason::DowngradedToReAct);
            }
            
            // 检查完成
            if self.check_completion() {
                return Ok(ExitReason::RalphLoopShipped);
            }
        }
        
        Ok(ExitReason::RalphLoopMaxIterations)
    }
    
    fn should_downgrade(&self, exit_reason: &ExitReason, iterations: u32) -> bool {
        if !self.config.auto_downgrade {
            return false;
        }
        
        // 退化条件
        if iterations <= self.config.downgrade_threshold {
            match exit_reason {
                ExitReason::NormalComplete => {
                    // 早期完成，退化为纯 ReAct
                    return true;
                }
                _ => {}
            }
        }
        
        false
    }
}
```

### 4.3 第三阶段：运行时退化

#### 步骤 3：实现运行时退化逻辑

```rust
impl HybridLoop {
    /// 运行时检查是否应该退化
    fn check_runtime_downgrade(&self, state: &LoopState) -> bool {
        if !self.config.auto_downgrade || self.is_downgraded {
            return false;
        }
        
        // 条件1：ReAct 在很少迭代内完成
        if state.react_steps_taken <= 2 && matches!(state.last_exit, ExitReason::NormalComplete) {
            tracing::info!(
                "Runtime downgrade: ReAct completed in {} steps",
                state.react_steps_taken
            );
            return true;
        }
        
        // 条件2：连续多次没有有效进展
        if state.no_progress_streak >= 3 {
            tracing::info!(
                "Runtime downgrade: {} iterations with no progress",
                state.no_progress_streak
            );
            return true;
        }
        
        // 条件3：任务被标记为简单，且已完成第一轮
        if state.complexity == TaskComplexity::Simple && state.ralph_iteration >= 1 {
            return true;
        }
        
        false
    }
}
```

### 4.4 第四阶段：集成

#### 步骤 4：集成到 Agent

```rust
impl Agent {
    pub async fn react(
        &self,
        session_config: SessionConfig,
        cancel_token: CancellationToken,
    ) -> Result<BoxStream<Result<AgentEvent>>> {
        let task = session_config.initial_task.as_deref().unwrap_or("");
        
        // 自动分析并选择模式
        let config = self.build_loop_config(&session_config, task);
        
        let mut loop_controller = HybridLoop::new(config, task);
        
        // 根据模式执行
        match loop_controller.run(task).await {
            Ok(exit_reason) => {
                // 处理退出
            }
            Err(e) => {
                // 处理错误
            }
        }
    }
    
    fn build_loop_config(&self, session_config: &SessionConfig, task: &str) -> LoopConfig {
        // 如果用户明确指定模式，使用用户配置
        if let Some(ref mode) = session_config.loop_mode {
            return LoopConfig {
                mode: mode.clone(),
                ..Default::default()
            };
        }
        
        // 否则使用自动模式（混合+退化）
        LoopConfig::default()
    }
}
```

---

## 五、配置使用示例

### 5.1 默认配置（推荐）

```rust
let config = LoopConfig {
    // 默认使用混合模式 + 自动退化
    mode: LoopMode::Hybrid(HybridConfig::default()),
    
    // 自动退化设置
    auto_downgrade: true,        // 开启自动退化
    downgrade_threshold: 3,       // 3次迭代内完成则退化
    
    // 其他设置
    max_steps: 50,
    enable_detection: true,
};

// 使用方式
let events = agent.react(session_config, cancel_token).await?;
```

### 5.2 强制使用 ReAct

```rust
let config = LoopConfig {
    mode: LoopMode::ReAct,
    auto_downgrade: false,
    ..Default::default()
};
```

### 5.3 强制使用 Hybrid（不退化）

```rust
let config = LoopConfig {
    mode: LoopMode::Hybrid(HybridConfig {
        ralph_max_iterations: 10,
        react_max_steps: 30,
        ..Default::default()
    }),
    auto_downgrade: false,  // 禁用退化
    ..Default::default()
};
```

---

## 六、工作流程图

```
┌─────────────────────────────────────────────────────────────────────┐
│                          完整工作流程                                     │
│                                                                      │
│  1. 接收任务                                                         │
│         │                                                            │
│         ▼                                                            │
│  2. TaskAnalyzer 分析任务                                             │
│         │                                                            │
│         ▼                                                            │
│  3. 简单任务? ──YES──▶ 退化模式 = ReAct                              │
│      │                                                               │
│      NO                                                              │
│      │                                                               │
│      ▼                                                               │
│  4. Hybrid 模式                                                       │
│      │                                                               │
│      ▼                                                               │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  for ralph_iter in 1..max_ralph_iterations:                 │  │
│  │      │                                                       │  │
│  │      ▼                                                       │  │
│  │  ┌───────────────────────────────────────────────────────┐  │  │
│  │  │  ReAct Loop (内层)                                    │  │  │
│  │  │  - 调用 LLM                                           │  │  │
│  │  │  - 执行工具                                            │  │  │
│  │  │  - 检查退出条件                                        │  │  │
│  │  │       │                                                │  │  │
│  │  │       ▼                                                │  │  │
│  │  │  ┌─────────────────────────────────────────────┐     │  │  │
│  │  │  │ 运行时退化检查:                                │     │  │  │
│  │  │  │ - 迭代次数 <= threshold?                     │     │  │  │
│  │  │  │ - 无工具调用完成?                              │     │  │  │
│  │  │  │ - 连续无进展?                                  │     │  │  │
│  │  │  │        │                                      │     │  │  │
│  │  │  │        ▼                                      │     │  │  │
│  │  │  │  YES ──▶ 退化为纯 ReAct，退出 Hybrid          │     │  │  │
│  │  │  │        │                                      │     │  │  │
│  │  │  │       NO                                      │     │  │  │
│  │  │  │        │                                      │     │  │  │
│  │  │  └────────┼──────────────────────────────────────┘  │  │  │
│  │  └──────────┼──────────────────────────────────────────────┘  │  │
│  │             │                                                    │  │
│  │             ▼                                                    │  │
│  │      检查外部完成条件                                             │  │
│  │             │                                                    │  │
│  │      ┌──────┴──────┐                                            │  │
│  │      │             │                                             │  │
│  │      ▼             ▼                                             │  │
│  │  SHIP?        继续下一轮                                          │  │
│  │                                                                  │  │
│  └───────────────────────────────────────────────────────────────┘  │
│         │                                                            │
│         ▼                                                            │
│  5. 返回 ExitReason                                                  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 七、验收标准

1. ✅ 混合模式正常工作
2. ✅ 任务分析器正确判断复杂度
3. ✅ 简单任务自动退化为 ReAct
4. ✅ 运行时退化机制正常工作
5. ✅ 状态文件正确持久化
6. ✅ 各种退出原因正确区分
7. ✅ 外部取消信号立即终止
8. ✅ 配置灵活可调
9. ✅ 单元测试覆盖核心逻辑
10. ✅ 与现有代码风格一致
