# 数据分析示例

## 1. 数据查询助手

### 1.1 SQL 生成

```python
def generate_sql(description: str, schema: str = None) -> dict:
    """根据自然语言生成 SQL"""
    message = f"""请根据以下描述生成 SQL 查询：

描述：{description}
"""
    
    if schema:
        message += f"""

数据库 Schema：
```
{schema}
```
"""
    
    message += """

要求：
1. 使用标准 SQL 语法
2. 添加必要的注释
3. 考虑性能优化
4. 处理可能的 NULL 值
"""
    
    return chat(message)

# 使用示例
schema = """
CREATE TABLE users (
    id INT PRIMARY KEY,
    name VARCHAR(100),
    email VARCHAR(100),
    created_at TIMESTAMP,
    status VARCHAR(20)
);

CREATE TABLE orders (
    id INT PRIMARY KEY,
    user_id INT,
    amount DECIMAL(10, 2),
    created_at TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id)
);
"""

result = generate_sql(
    "查询过去30天内每个用户的订单总金额，只包含活跃用户",
    schema
)
print(result['message']['content'])
```

### 1.2 查询优化

```python
def optimize_sql(query: str, schema: str = None) -> dict:
    """优化 SQL 查询"""
    message = f"""请优化以下 SQL 查询：

```sql
{query}
```
"""
    
    if schema:
        message += f"""

数据库 Schema：
```
{schema}
```
"""
    
    message += """

优化方向：
1. 索引建议
2. 查询重写
3. 执行计划分析
4. 性能对比
"""
    
    return chat(message)

# 使用示例
query = """
SELECT u.name, COUNT(o.id) as order_count
FROM users u
LEFT JOIN orders o ON u.id = o.user_id
WHERE u.created_at > '2024-01-01'
GROUP BY u.name
HAVING COUNT(o.id) > 10
ORDER BY order_count DESC
"""

result = optimize_sql(query, schema)
print(result['message']['content'])
```

## 2. 数据可视化助手

### 2.1 图表生成

```python
def generate_chart_code(
    data_description: str,
    chart_type: str,
    library: str = 'matplotlib'
) -> dict:
    """生成图表代码"""
    message = f"""请生成{library}代码来创建{chart_type}图表：

数据描述：{data_description}

要求：
1. 使用{library}库
2. 包含标题和标签
3. 美观的样式
4. 保存为图片文件
"""
    
    return chat(message)

# 使用示例
result = generate_chart_code(
    "过去12个月的销售数据，包含月份和销售额",
    "折线图",
    "matplotlib"
)
print(result['message']['content'])
```

### 2.2 仪表板生成

```python
def generate_dashboard(
    metrics: list,
    layout: str = 'grid'
) -> dict:
    """生成仪表板代码"""
    message = f"""请生成一个数据仪表板：

指标：
{chr(10).join(f'- {m}' for m in metrics)}

布局：{layout}

要求：
1. 使用 Streamlit 或 Dash
2. 交互式图表
3. 实时数据更新
4. 响应式布局
"""
    
    return chat(message)

# 使用示例
metrics = [
    "日活跃用户数",
    "订单转化率",
    "平均订单金额",
    "用户留存率"
]

result = generate_dashboard(metrics)
print(result['message']['content'])
```

## 3. 数据处理助手

### 3.1 数据清洗

```python
def generate_cleaning_code(
    data_description: str,
    issues: list
) -> dict:
    """生成数据清洗代码"""
    message = f"""请生成数据清洗代码：

数据描述：{data_description}

问题：
{chr(10).join(f'- {issue}' for issue in issues)}

要求：
1. 使用 pandas
2. 处理缺失值
3. 处理异常值
4. 数据类型转换
5. 生成清洗报告
"""
    
    return chat(message)

# 使用示例
result = generate_cleaning_code(
    "用户信息表，包含姓名、邮箱、年龄、注册时间",
    [
        "姓名字段有空格",
        "邮箱格式不统一",
        "年龄有负数和异常大值",
        "注册时间格式不一致"
    ]
)
print(result['message']['content'])
```

### 3.2 数据转换

```python
def generate_transformation_code(
    source_schema: str,
    target_schema: str,
    transformations: list
) -> dict:
    """生成数据转换代码"""
    message = f"""请生成数据转换代码：

源 Schema：
```
{source_schema}
```

目标 Schema：
```
{target_schema}
```

转换规则：
{chr(10).join(f'- {t}' for t in transformations)}

要求：
1. 使用 pandas 或 SQL
2. 数据验证
3. 错误处理
4. 性能优化
"""
    
    return chat(message)

# 使用示例
result = generate_transformation_code(
    "users(id, name, email, created_at)",
    "customers(customer_id, full_name, email_address, registration_date)",
    [
        "id -> customer_id",
        "name -> full_name",
        "email -> email_address",
        "created_at -> registration_date (格式转换)"
    ]
)
print(result['message']['content'])
```

## 4. 数据报告助手

### 4.1 报告生成

```python
def generate_report(
    data_summary: str,
    report_type: str = 'summary'
) -> dict:
    """生成数据分析报告"""
    message = f"""请生成数据分析报告：

数据摘要：
{data_summary}

报告类型：{report_type}

要求：
1. 执行摘要
2. 数据概览
3. 关键发现
4. 趋势分析
5. 建议和行动项
6. Markdown 格式
"""
    
    return chat(message)

# 使用示例
data_summary = """
总用户数：10,000
活跃用户：7,500
平均会话时长：15分钟
转化率：3.5%
主要用户来源：搜索(40%)、直接访问(30%)、推荐(30%)
"""

result = generate_report(data_summary, 'monthly')
print(result['message']['content'])
```

### 4.2 洞察提取

```python
def extract_insights(
    data_description: str,
    analysis_results: str
) -> dict:
    """从分析结果中提取洞察"""
    message = f"""请从以下分析结果中提取关键洞察：

数据描述：{data_description}

分析结果：
{analysis_results}

要求：
1. 识别关键模式
2. 发现异常情况
3. 提出假设
4. 给出建议
5. 优先级排序
"""
    
    return chat(message)

# 使用示例
analysis_results = """
用户增长趋势：
- 1月：+5%
- 2月：+3%
- 3月：+8%
- 4月：+15%
- 5月：+12%

转化率趋势：
- 1月：2.5%
- 2月：2.8%
- 3月：3.2%
- 4月：3.5%
- 5月：3.5%
"""

result = extract_insights("过去5个月的用户和转化数据", analysis_results)
print(result['message']['content'])
```

## 5. 机器学习助手

### 5.1 特征工程

```python
def generate_feature_engineering_code(
    data_description: str,
    target_variable: str,
    problem_type: str
) -> dict:
    """生成特征工程代码"""
    message = f"""请生成特征工程代码：

数据描述：{data_description}

目标变量：{target_variable}

问题类型：{problem_type}

要求：
1. 特征选择
2. 特征转换
3. 特征创建
4. 特征缩放
5. 使用 scikit-learn
"""
    
    return chat(message)

# 使用示例
result = generate_feature_engineering_code(
    "用户行为数据，包含浏览历史、购买记录、点击流",
    "是否购买",
    "二分类"
)
print(result['message']['content'])
```

### 5.2 模型训练

```python
def generate_model_training_code(
    features: list,
    target: str,
    model_type: str
) -> dict:
    """生成模型训练代码"""
    message = f"""请生成模型训练代码：

特征：{', '.join(features)}

目标变量：{target}

模型类型：{model_type}

要求：
1. 数据分割
2. 交叉验证
3. 超参数调优
4. 模型评估
5. 模型保存
"""
    
    return chat(message)

# 使用示例
result = generate_model_training_code(
    ["age", "income", "education", "location"],
    "purchase_probability",
    "随机森林"
)
print(result['message']['content'])
```

## 6. 相关文档

- [基础聊天示例](basic-chat.md)
- [工具开发指南](../guides/tool-development.md)
