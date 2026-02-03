# 代码助手示例

## 1. 代码审查助手

### 1.1 Python 实现

```python
import os
from pathlib import Path

def review_code(file_path: str, session_id: str = None) -> dict:
    """审查代码文件"""
    with open(file_path, 'r') as f:
        code = f.read()
    
    message = f"""请审查以下代码文件：{file_path}

```{get_language(file_path)}
{code}
```

请关注以下方面：
1. 代码风格和格式
2. 潜在的 bug 和错误
3. 性能问题
4. 安全漏洞
5. 可维护性
"""
    
    return chat(message, session_id)

def get_language(file_path: str) -> str:
    """根据文件扩展名获取语言"""
    ext = Path(file_path).suffix
    lang_map = {
        '.py': 'python',
        '.js': 'javascript',
        '.ts': 'typescript',
        '.rs': 'rust',
        '.go': 'go',
        '.java': 'java',
        '.cpp': 'cpp',
        '.c': 'c',
    }
    return lang_map.get(ext, '')

# 使用示例
result = review_code('src/main.py')
print(result['message']['content'])
```

### 1.2 批量审查

```python
def review_project(project_dir: str) -> list:
    """审查整个项目"""
    results = []
    session_id = None
    
    for root, dirs, files in os.walk(project_dir):
        # 跳过隐藏目录和虚拟环境
        dirs[:] = [d for d in dirs if not d.startswith('.') and d != 'venv']
        
        for file in files:
            if file.endswith(('.py', '.js', '.ts', '.rs', '.go')):
                file_path = os.path.join(root, file)
                print(f"Reviewing: {file_path}")
                
                result = review_code(file_path, session_id)
                session_id = result['session_id']
                
                results.append({
                    'file': file_path,
                    'review': result['message']['content']
                })
    
    return results

# 使用示例
reviews = review_project('my-project')
for review in reviews:
    print(f"\n{review['file']}:")
    print(review['review'])
```

## 2. 代码生成助手

### 2.1 函数生成

```python
def generate_function(description: str, language: str = 'python') -> dict:
    """根据描述生成函数"""
    message = f"""请生成一个{language}函数：

描述：{description}

要求：
1. 包含类型注解
2. 包含文档字符串
3. 包含错误处理
4. 遵循{language}最佳实践
"""
    
    return chat(message)

# 使用示例
result = generate_function(
    "计算两个日期之间的工作日数量",
    "python"
)
print(result['message']['content'])
```

### 2.2 测试生成

```python
def generate_tests(code: str, language: str = 'python') -> dict:
    """为代码生成测试"""
    message = f"""请为以下{language}代码生成单元测试：

```{language}
{code}
```

要求：
1. 测试正常情况
2. 测试边界条件
3. 测试错误处理
4. 使用pytest框架
5. 覆盖率目标：80%+
"""
    
    return chat(message)

# 使用示例
code = '''
def divide(a: float, b: float) -> float:
    """除法运算"""
    if b == 0:
        raise ValueError("Division by zero")
    return a / b
'''

result = generate_tests(code)
print(result['message']['content'])
```

### 2.3 文档生成

```python
def generate_docs(code: str, language: str = 'python') -> dict:
    """为代码生成文档"""
    message = f"""请为以下{language}代码生成文档：

```{language}
{code}
```

要求：
1. 生成README.md
2. 包含安装说明
3. 包含使用示例
4. 包含API文档
5. 包含贡献指南
"""
    
    return chat(message)

# 使用示例
result = generate_docs(code)
print(result['message']['content'])
```

## 3. 代码重构助手

### 3.1 代码优化

```python
def optimize_code(code: str, language: str = 'python') -> dict:
    """优化代码"""
    message = f"""请优化以下{language}代码：

```{language}
{code}
```

优化方向：
1. 性能优化
2. 内存优化
3. 可读性优化
4. 可维护性优化

请提供优化后的代码和优化说明。
"""
    
    return chat(message)

# 使用示例
code = '''
def find_duplicates(items):
    duplicates = []
    for i in range(len(items)):
        for j in range(i + 1, len(items)):
            if items[i] == items[j] and items[i] not in duplicates:
                duplicates.append(items[i])
    return duplicates
'''

result = optimize_code(code)
print(result['message']['content'])
```

### 3.2 代码现代化

```python
def modernize_code(code: str, language: str = 'python') -> dict:
    """现代化代码"""
    message = f"""请将以下{language}代码现代化：

```{language}
{code}
```

现代化方向：
1. 使用最新语法特性
2. 使用现代库和框架
3. 改进类型注解
4. 应用最佳实践

请提供现代化后的代码和改进说明。
"""
    
    return chat(message)

# 使用示例
code = '''
def process_data(data):
    result = []
    for item in data:
        if item['active']:
            result.append(item['value'])
    return result
'''

result = modernize_code(code)
print(result['message']['content'])
```

## 4. 代码调试助手

### 4.1 错误诊断

```python
def diagnose_error(code: str, error: str, language: str = 'python') -> dict:
    """诊断代码错误"""
    message = f"""请诊断以下{language}代码的错误：

代码：
```{language}
{code}
```

错误信息：
```
{error}
```

请提供：
1. 错误原因分析
2. 修复建议
3. 修复后的代码
4. 预防措施
"""
    
    return chat(message)

# 使用示例
code = '''
def calculate_average(numbers):
    return sum(numbers) / len(numbers)
'''

error = '''
TypeError: unsupported operand type(s) for /: 'int' and 'NoneType'
'''

result = diagnose_error(code, error)
print(result['message']['content'])
```

### 4.2 性能分析

```python
def analyze_performance(code: str, language: str = 'python') -> dict:
    """分析代码性能"""
    message = f"""请分析以下{language}代码的性能：

```{language}
{code}
```

分析方向：
1. 时间复杂度分析
2. 空间复杂度分析
3. 潜在性能瓶颈
4. 优化建议

请提供详细的分析报告。
"""
    
    return chat(message)

# 使用示例
code = '''
def fibonacci(n):
    if n <= 1:
        return n
    return fibonacci(n - 1) + fibonacci(n - 2)
'''

result = analyze_performance(code)
print(result['message']['content'])
```

## 5. TypeScript 实现

### 5.1 代码审查

```typescript
import fs from 'fs';
import path from 'path';

async function reviewCode(
  filePath: string,
  sessionId?: string
): Promise<ChatResponse> {
  const code = fs.readFileSync(filePath, 'utf-8');
  const language = getLanguage(filePath);
  
  const message = `请审查以下代码文件：${filePath}

\`\`\`${language}
${code}
\`\`\`

请关注以下方面：
1. 代码风格和格式
2. 潜在的 bug 和错误
3. 性能问题
4. 安全漏洞
5. 可维护性
`;
  
  return await chat(message, sessionId);
}

function getLanguage(filePath: string): string {
  const ext = path.extname(filePath);
  const langMap: Record<string, string> = {
    '.py': 'python',
    '.js': 'javascript',
    '.ts': 'typescript',
    '.rs': 'rust',
    '.go': 'go',
    '.java': 'java',
    '.cpp': 'cpp',
    '.c': 'c',
  };
  return langMap[ext] || '';
}

// 使用示例
async function main() {
  const result = await reviewCode('src/main.ts');
  console.log(result.message.content);
}

main();
```

## 6. 相关文档

- [基础聊天示例](basic-chat.md)
- [工具开发指南](../guides/tool-development.md)
- [技能编写指南](../guides/skill-writing.md)
