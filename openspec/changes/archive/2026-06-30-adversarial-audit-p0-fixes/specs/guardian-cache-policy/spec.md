## ADDED Requirements

### Requirement: Guardian Reviewer Must Carry Cache Policy

guardian reviewer 的所有 LLM 调用路径 MUST 携带 `Some(CachePolicy)`，禁止 `cache_policy: None`，以使请求能命中 Anthropic Prompt Caching。

#### Scenario: Guardian Review Request Has Cache Policy

- **WHEN** guardian reviewer 发起 LLM 审查请求
- **THEN** 请求 MUST 携带 `Some(CachePolicy::default())` 或更细粒度的 policy，使 provider transform 层能注入 `cache_control` hint

#### Scenario: E2E Test Path Has Cache Policy

- **WHEN** e2e_llm_test.rs 构造 LLM 请求
- **THEN** 请求 MUST 携带 `Some(CachePolicy::default())`，与生产路径行为一致

#### Scenario: Cache Policy Reduces Cost

- **WHEN** guardian 在一个 session 内触发多次审查
- **THEN** 后续审查请求 MUST 能命中前序请求的 prompt cache，input token 成本从 $3.00/M 降至 $0.30/M
