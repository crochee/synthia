# prefix-hash-scope Specification

## Purpose
TBD - created by archiving change adversarial-audit-p0-fixes. Update Purpose after archive.
## Requirements
### Requirement: PrefixTracker Hash Must Cover Full Prefix

PrefixTracker 计算 prefix hash 时 MUST 覆盖完整的 prefix：system_prompt + tools_schema + messages 前缀（`tool_result_cleared_at` 之前的部分），禁止只 hash system_prompt。

#### Scenario: Tools Change Detected

- **WHEN** system_prompt 未变但 tools schema 变更（如新增/移除工具）
- **THEN** PrefixTracker 的 prefix hash MUST 改变，`stability_ratio` 反映该变更

#### Scenario: Messages Prefix Change Detected

- **WHEN** system_prompt 与 tools 未变但 messages 前缀变更（如清理了 tool_result_cleared_at 之前的内容）
- **THEN** PrefixTracker 的 prefix hash MUST 改变

#### Scenario: Stable Prefix Reported Correctly

- **WHEN** system_prompt + tools + messages 前缀三者均未变
- **THEN** `stability_ratio` MUST 准确反映 prefix 稳定性（接近 1.0），不虚高

#### Scenario: Hash Input Concatenated In Deterministic Order

- **WHEN** 构造 hash 输入
- **THEN** 输入 MUST 按 system_bytes || tools_schema_bytes || messages_prefix_bytes 顺序拼接，序列化 MUST 确定性（JSON key 排序、空格规范化）

