## Purpose

Layer observability on top of the existing Prometheus port: per-step `Context Trace` files, eight key indicators (prefix stability, cache hit ratio, pruning stage distribution, loop blocks, etc.), and a local alerting channel. This gives operators actionable signals when context or loop behavior deviates from expected.

## Requirements

### Requirement: Context Trace SHALL be recorded for each API call
Before each LLM API call, the system SHALL record a trace entry containing: timestamp, session_id, step, message_count, total_tokens, context_utilization, prefix_hash, prefix_changed, cache_hit, pruning_stage, and sections distribution. Each trace entry SHALL be written to a separate file `~/.synthia/traces/context_<session_id>_<step>.jsonl`.

#### Scenario: Trace recorded after model call
- **WHEN** the agent calls the LLM model
- **THEN** a trace file SHALL be created with all required fields

### Requirement: Prometheus metrics SHALL expose key indicators
The system SHALL expose the following Prometheus metrics: prefix_stability_ratio (target >85%), cache_hit_ratio (target >85%), pruning_stage_distribution (target 80%+ Stage 1), loop_detection_blocks (target 趋近 0), context_utilization (target 40-70% uniform), tool_timeout_count (target 趋近 0), tool_retry_count (target 趋近 0), cron_execution_success_rate (target >95%), memory_search_latency_ms (target <500ms).

#### Scenario: Metrics endpoint returns all indicators
- **WHEN** Prometheus scrapes the metrics port (default 9090)
- **THEN** all 9 metrics SHALL be present with current values

### Requirement: Local alerts SHALL be emitted for critical conditions
The system SHALL emit local alerts (CLI output) for: prefix_hash changed 3+ consecutive times (WARNING), context_utilization > 90% (CRITICAL), context_utilization < HARD_MIN (CRITICAL), loop_detection_blocks > 0 (WARNING), tool_timeout_count > 5 (WARNING), cron job failed 2+ consecutive times (WARNING).

#### Scenario: Critical alert when context nearly exhausted
- **WHEN** context_utilization exceeds 90%
- **THEN** the system SHALL output a CRITICAL alert to the CLI

### Requirement: Trace files SHALL use per-step independent naming
Each trace entry SHALL be written to an independent file named `context_<session_id>_<step>.jsonl` to avoid concurrent write race conditions between main agent and subagents.

#### Scenario: Concurrent traces do not corrupt each other
- **WHEN** main agent and subagent both write traces simultaneously
- **THEN** each trace SHALL be written to its own file without corruption
