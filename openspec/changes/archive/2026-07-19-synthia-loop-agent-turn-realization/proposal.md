# Proposal: synthia-loop-agent-turn-realization

> Change #2 — Loop/Agent/Turn 真化: 消费 change #1 基础设施，集成到 main_loop

## Why

Change #1 交付了 8 个基础设施 capability（EventV2、Extension、ServiceRegistry、GoalService、Hook 统一、Tool materialization、Output sanitizer、Custom event renderer），但这些基础设施 **定义而未消费**——main_loop 仍然使用旧的 `HookBuilder` 4 方法调用，`ForwardToMainAgent` 从未被任何代码处理，`LoopDetector` 未接入主循环，`GoalService` 未接入运行时准入控制。

同时，`main_loop.rs` 已达 1077 行，`AgentRunConfig` 的 11+ 字段在 `run_with_steps` 中被解构后丢弃（`_` 前缀），`LoopServices` 缺少 `goal` 和 `extension_registry` 字段。`ExtensionOutcome` 与 `HookOutcome` 是两个独立定义但结构相同的 3-state enum，无类型级桥接。

Change #2 的目标是将 change #1 的基础设施 **消费** 到 main_loop 中，使这些 capability 真正生效。

## What Changes

1. **UnifiedHookDispatcher** — 替代 `HookBuilder`，统一分发 `HookEvent` 到 `HookRegistry` + `ExtensionRegistry`，合并 `HookOutcome` 与 `ExtensionOutcome`（通过 `From` 转换）
2. **ForwardToMainAgent 消费** — 子代理 hook/extension 返回 `ForwardToMainAgent` 时，注入到父代理的 `SteeringChannel`（新增 `SteeringPriority::Forwarded` 优先级）
3. **LoopDetector 分层集成** — `LoopDetectorSet` 作为硬底线（不可覆盖），`synthia-hook::LoopDetector` 作为软 hook 层（可配置）
4. **GoalService 准入控制** — `LoopServices` 新增 `goal_service` + `goal_tracker` 字段，main_loop 在 turn 开始时做 admission gate
5. **Extension 事件触发** — 19 个 extension 事件全部接入 main_loop 生命周期点（session start/end, pre/post LLM, pre/post tool, pre/post compact, pre/post steering, subagent spawn 等）
6. **AgentRunConfig 字段提升** — 5 个运行时需要的字段从 config 提升到 `LoopServices`，6 个一次性构造参数保留在 config
7. **ExtensionRegistry → ServiceRegistry 双注册修复** — `ExtensionRegistry::register()` 补充 `ServiceRegistry` 注册调用

## Capabilities

### New Capabilities

| Capability | Description |
|------------|-------------|
| `unified-hook-dispatcher` | UnifiedHookDispatcher + From<ExtensionOutcome> bridge + hook-first ordering + combined outcome resolution |
| `forward-to-main-agent` | ForwardToMainAgent 消费路径：SteeringChannel 注入 + SteeringPriority::Forwarded + rate limiting |
| `loop-detector-layered-integration` | Layered loop detection: LoopDetectorSet (hard floor) + synthia-hook::LoopDetector (soft vote) + integration in fire_before_tool |
| `goal-service-admission` | GoalService admission gate in main_loop + LoopServices fields + turn-level semaphore |
| `extension-event-wiring` | 19 extension events wired into main_loop lifecycle + ExtensionRegistry double-registration fix |

### Modified Capabilities

| Capability | Change |
|------------|--------|
| `hook-system-unification` | HookBuilder deprecated in favor of UnifiedHookDispatcher; fire_* methods gain `#[deprecated]` |

## Impact

- **Code**: `main_loop.rs` (1077→~850 行 after extraction), `LoopServices` (+5 fields), `HookBuilder` (deprecated), `BuilderSteps` (hook_dispatcher replaces hooks), new `UnifiedHookDispatcher` (~200 行), `steering.rs` (+1 priority level)
- **API**: `HookBuilder::fire_*` → `#[deprecated]` (6-month window), new `UnifiedHookDispatcher::dispatch()` public API
- **Dependencies**: `synthia-extension-v2` gains `synthia-hook` dep (for `From<ExtensionOutcome> for HookOutcome`)
- **Backward compatibility**: Old `HookBuilder` path preserved behind feature flag for 6-month deprecation window; new `UnifiedHookDispatcher` is opt-in via `LoopServices`
- **Runtime behavior**: Hook dispatch order changes (hook-first, then extension); ForwardToMainAgent now actually routes messages; GoalService can reject turn admission
