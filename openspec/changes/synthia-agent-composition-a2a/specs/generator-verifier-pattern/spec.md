## ADDED Requirements

### Requirement: GeneratorVerifier struct
GeneratorVerifier 组合两个 AgentHandle：
- generator: Arc<AgentHandle> — 生成器
- verifier: Arc<AgentHandle> — 验证器
- max_rounds: usize — 最大循环次数
- pass_fn: fn(&str) -> bool — 判定 PASS 的函数

### Requirement: GeneratorVerifier.run
run(task) 语义：
1. gen_tool = agent_as_tool(generator)
2. ver_tool = agent_as_tool(verifier)
3. loop max_rounds:
   a. output = gen_tool.call(task + feedback)
   b. verdict = ver_tool.call(output)
   c. if pass_fn(verdict): return Ok(output)
   d. feedback = verdict
4. Err(MaxRoundsExceeded)

### Requirement: GeneratorVerifier supports A2A
generator 和 verifier 可以是远程 agent（通过 SendMessage Tool），不限于本地 AgentHandle。
