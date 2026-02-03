# 多Agent工作流示例

## 1. 代码审查工作流

### 1.1 工作流设计

```python
def code_review_workflow(file_path: str) -> dict:
    """完整的代码审查工作流"""
    
    # 步骤1：代码审查
    print("Step 1: Code Review")
    review_result = chat(
        f"请审查代码文件：{file_path}",
        agent="code-reviewer"
    )
    
    # 步骤2：安全审查
    print("Step 2: Security Review")
    security_result = chat(
        f"请进行安全审查：\n{review_result['message']['content']}",
        agent="security-reviewer",
        session_id=review_result['session_id']
    )
    
    # 步骤3：性能审查
    print("Step 3: Performance Review")
    performance_result = chat(
        f"请进行性能审查：\n{review_result['message']['content']}",
        agent="performance-reviewer",
        session_id=security_result['session_id']
    )
    
    # 步骤4：生成综合报告
    print("Step 4: Generate Report")
    report_result = chat(
        f"""请生成综合审查报告：

代码审查：
{review_result['message']['content']}

安全审查：
{security_result['message']['content']}

性能审查：
{performance_result['message']['content']}
""",
        session_id=performance_result['session_id']
    )
    
    return {
        'review': review_result['message']['content'],
        'security': security_result['message']['content'],
        'performance': performance_result['message']['content'],
        'report': report_result['message']['content']
    }

# 使用示例
result = code_review_workflow('src/main.py')
print("\n=== 综合报告 ===")
print(result['report'])
```

### 1.2 并行审查

```python
import asyncio

async def parallel_code_review(file_path: str) -> dict:
    """并行代码审查"""
    
    # 读取代码
    with open(file_path, 'r') as f:
        code = f.read()
    
    # 并行执行三个审查
    tasks = [
        asyncio.create_task(
            async_chat(
                f"请审查代码：\n```\n{code}\n```",
                agent="code-reviewer"
            )
        ),
        asyncio.create_task(
            async_chat(
                f"请进行安全审查：\n```\n{code}\n```",
                agent="security-reviewer"
            )
        ),
        asyncio.create_task(
            async_chat(
                f"请进行性能审查：\n```\n{code}\n```",
                agent="performance-reviewer"
            )
        ),
    ]
    
    results = await asyncio.gather(*tasks)
    
    # 生成综合报告
    report = await async_chat(
        f"""请生成综合审查报告：

代码审查：{results[0]['message']['content']}
安全审查：{results[1]['message']['content']}
性能审查：{results[2]['message']['content']}
"""
    )
    
    return {
        'review': results[0]['message']['content'],
        'security': results[1]['message']['content'],
        'performance': results[2]['message']['content'],
        'report': report['message']['content']
    }

# 使用示例
result = asyncio.run(parallel_code_review('src/main.py'))
```

## 2. 测试生成工作流

### 2.1 完整测试流程

```python
def test_generation_workflow(code: str) -> dict:
    """完整的测试生成工作流"""
    
    # 步骤1：分析代码
    print("Step 1: Analyze Code")
    analysis_result = chat(
        f"请分析以下代码的结构和功能：\n```\n{code}\n```"
    )
    
    # 步骤2：生成单元测试
    print("Step 2: Generate Unit Tests")
    unit_tests = chat(
        f"""根据代码分析生成单元测试：

代码分析：
{analysis_result['message']['content']}

原始代码：
```
{code}
```
""",
        agent="test-generator",
        session_id=analysis_result['session_id']
    )
    
    # 步骤3：生成集成测试
    print("Step 3: Generate Integration Tests")
    integration_tests = chat(
        f"请生成集成测试：\n{analysis_result['message']['content']}",
        agent="test-generator",
        session_id=unit_tests['session_id']
    )
    
    # 步骤4：验证测试覆盖率
    print("Step 4: Verify Coverage")
    coverage_check = chat(
        f"""请检查测试覆盖率：

单元测试：
{unit_tests['message']['content']}

集成测试：
{integration_tests['message']['content']}

原始代码：
```
{code}
```
""",
        session_id=integration_tests['session_id']
    )
    
    return {
        'analysis': analysis_result['message']['content'],
        'unit_tests': unit_tests['message']['content'],
        'integration_tests': integration_tests['message']['content'],
        'coverage': coverage_check['message']['content']
    }

# 使用示例
code = '''
def calculate_discount(price: float, customer_type: str) -> float:
    """计算折扣价格"""
    discounts = {
        'regular': 0.0,
        'silver': 0.1,
        'gold': 0.2,
        'platinum': 0.3
    }
    
    if customer_type not in discounts:
        raise ValueError(f"Invalid customer type: {customer_type}")
    
    return price * (1 - discounts[customer_type])
'''

result = test_generation_workflow(code)
print(result['unit_tests'])
```

## 3. 文档生成工作流

### 3.1 完整文档流程

```python
def documentation_workflow(code: str, project_name: str) -> dict:
    """完整的文档生成工作流"""
    
    # 步骤1：分析代码结构
    print("Step 1: Analyze Code Structure")
    structure_result = chat(
        f"请分析代码结构和模块：\n```\n{code}\n```"
    )
    
    # 步骤2：生成 API 文档
    print("Step 2: Generate API Docs")
    api_docs = chat(
        f"请生成 API 文档：\n{structure_result['message']['content']}",
        agent="documentation-writer",
        session_id=structure_result['session_id']
    )
    
    # 步骤3：生成用户指南
    print("Step 3: Generate User Guide")
    user_guide = chat(
        f"请生成用户指南：\n{structure_result['message']['content']}",
        agent="documentation-writer",
        session_id=api_docs['session_id']
    )
    
    # 步骤4：生成 README
    print("Step 4: Generate README")
    readme = chat(
        f"""请为项目 {project_name} 生成 README.md：

API 文档：
{api_docs['message']['content']}

用户指南：
{user_guide['message']['content']}
""",
        session_id=user_guide['session_id']
    )
    
    # 步骤5：生成示例代码
    print("Step 5: Generate Examples")
    examples = chat(
        f"请生成使用示例：\n{api_docs['message']['content']}",
        agent="documentation-writer",
        session_id=readme['session_id']
    )
    
    return {
        'structure': structure_result['message']['content'],
        'api_docs': api_docs['message']['content'],
        'user_guide': user_guide['message']['content'],
        'readme': readme['message']['content'],
        'examples': examples['message']['content']
    }

# 使用示例
result = documentation_workflow(code, "MyAwesomeLib")
print(result['readme'])
```

## 4. 持续集成工作流

### 4.1 CI/CD 流程

```python
def cicd_workflow(repo_path: str) -> dict:
    """CI/CD 工作流"""
    
    results = {}
    
    # 步骤1：代码检查
    print("Step 1: Code Check")
    results['lint'] = chat(
        f"请检查代码风格和格式：{repo_path}",
        agent="code-reviewer"
    )
    
    # 步骤2：安全扫描
    print("Step 2: Security Scan")
    results['security'] = chat(
        "请进行安全扫描",
        agent="security-reviewer",
        session_id=results['lint']['session_id']
    )
    
    # 步骤3：测试运行
    print("Step 3: Run Tests")
    results['test'] = chat(
        "请运行测试并生成报告",
        agent="test-runner",
        session_id=results['security']['session_id']
    )
    
    # 步骤4：构建检查
    print("Step 4: Build Check")
    results['build'] = chat(
        "请检查构建是否成功",
        session_id=results['test']['session_id']
    )
    
    # 步骤5：生成报告
    print("Step 5: Generate Report")
    results['report'] = chat(
        f"""请生成 CI/CD 报告：

代码检查：{results['lint']['message']['content']}
安全扫描：{results['security']['message']['content']}
测试结果：{results['test']['message']['content']}
构建结果：{results['build']['message']['content']}
""",
        session_id=results['build']['session_id']
    )
    
    return results

# 使用示例
result = cicd_workflow('/path/to/repo')
print(result['report']['message']['content'])
```

## 5. TypeScript 实现

### 5.1 工作流管理器

```typescript
interface WorkflowStep {
  name: string;
  agent?: string;
  action: (context: WorkflowContext) => Promise<any>;
}

interface WorkflowContext {
  sessionId?: string;
  data: Record<string, any>;
}

class WorkflowManager {
  private steps: WorkflowStep[] = [];
  private context: WorkflowContext = { data: {} };

  addStep(step: WorkflowStep): this {
    this.steps.push(step);
    return this;
  }

  async execute(): Promise<Record<string, any>> {
    const results: Record<string, any> = {};

    for (const step of this.steps) {
      console.log(`Executing: ${step.name}`);
      
      const result = await step.action(this.context);
      results[step.name] = result;
      this.context.data[step.name] = result;
      
      if (result.sessionId) {
        this.context.sessionId = result.sessionId;
      }
    }

    return results;
  }
}

// 使用示例
async function runCodeReviewWorkflow(filePath: string) {
  const workflow = new WorkflowManager();

  workflow
    .addStep({
      name: 'code-review',
      agent: 'code-reviewer',
      action: async (ctx) => {
        return await chat(`请审查代码：${filePath}`, {
          agent: 'code-reviewer',
          sessionId: ctx.sessionId,
        });
      },
    })
    .addStep({
      name: 'security-review',
      agent: 'security-reviewer',
      action: async (ctx) => {
        const review = ctx.data['code-review'];
        return await chat(
          `请进行安全审查：\n${review.message.content}`,
          {
            agent: 'security-reviewer',
            sessionId: ctx.sessionId,
          }
        );
      },
    })
    .addStep({
      name: 'generate-report',
      action: async (ctx) => {
        const codeReview = ctx.data['code-review'];
        const securityReview = ctx.data['security-review'];
        
        return await chat(
          `请生成综合报告：
          
代码审查：${codeReview.message.content}
安全审查：${securityReview.message.content}`,
          { sessionId: ctx.sessionId }
        );
      },
    });

  const results = await workflow.execute();
  return results;
}

// 执行工作流
runCodeReviewWorkflow('src/main.ts').then(console.log);
```

## 6. 相关文档

- [多Agent协作](../guides/multi-agent-collaboration.md)
- [工具开发指南](../guides/tool-development.md)
- [技能编写指南](../guides/skill-writing.md)
