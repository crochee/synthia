# prompt-assembly-linearity Specification

## Purpose
TBD - created by archiving change adversarial-audit-p0-fixes. Update Purpose after archive.
## Requirements
### Requirement: Prompt Assembly ProtectionZone Trim Must Be O(n)

prompt assembly 的 ProtectionZone trim 操作 MUST 为 O(n) 复杂度，禁止使用 `Vec::remove(0)` 这类 O(n) 单次操作在循环中调用 n 次（导致 O(n²)）。

#### Scenario: Large Message List Trims In Linear Time

- **WHEN** messages 数组有 200+ 条目且 total_tokens 超过 max_tokens
- **THEN** ProtectionZone trim 操作 MUST 在 O(n) 时间内完成，禁止 O(n²)

#### Scenario: Drain Replaces Remove(0)

- **WHEN** 实现ProtectionZone trim
- **THEN** 代码 MUST 使用 `Vec::drain(start..end)` 一次性移除或维护起始索引，禁止 `Vec::remove(0)` 循环

#### Scenario: Trim Semantics Preserved

- **WHEN** trim 操作执行
- **THEN** 保护的 messages（protected_messages）语义 MUST 保持不变：从最旧的消息开始移除直到 total_tokens <= max_tokens，保留最近 1 条

