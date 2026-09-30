# SERVER-ROUTES-TODO

Backend route inventory for the four management domains the
frontend surfaces. **All 18 routes below landed** in the
"前端贯通 backend" turn (2026-09-23); this file now records the
wire contract + what remains deferred.

## Landed routes

| Path                               | Crate               | Handler (`crates/synthia-server/src/routes/`) | Shape                                    |
| ---------------------------------- | ------------------- | --------------------------------------------- | ---------------------------------------- |
| `GET /api/v1/schedules`            | `synthia-scheduler` | `schedules::list_schedules`                   | `List<ScheduleSummary>`                  |
| `GET /api/v1/schedules/{id}`       | `synthia-scheduler` | `schedules::get_schedule`                     | `Schedule` (flattened `Job`)             |
| `POST /api/v1/schedules`           | `synthia-scheduler` | `schedules::create_schedule`                  | body `CreateScheduleRequest` → 201 `Job` |
| `PATCH /api/v1/schedules/{id}`     | `synthia-scheduler` | `schedules::patch_schedule`                   | body `UpdateScheduleRequest` → `Job`     |
| `DELETE /api/v1/schedules/{id}`    | `synthia-scheduler` | `schedules::delete_schedule`                  | 204                                      |
| `POST /api/v1/schedules/{id}/tick` | `synthia-scheduler` | `schedules::tick_schedule`                    | `ScheduleTickResponse`                   |
| `GET /api/v1/workflows`            | `synthia-workflow`  | `workflows::list_workflows`                   | `List<WorkflowSummary>`                  |
| `GET /api/v1/workflows/{id}`       | `synthia-workflow`  | `workflows::get_workflow`                     | `WorkflowSpec`                           |
| `POST /api/v1/workflows`           | `synthia-workflow`  | `workflows::create_workflow`                  | body `WorkflowSpec` → 201                |
| `PUT /api/v1/workflows/{id}`       | `synthia-workflow`  | `workflows::replace_workflow`                | body `WorkflowSpec` → `WorkflowSpec`     |
| `DELETE /api/v1/workflows/{id}`    | `synthia-workflow`  | `workflows::delete_workflow`                  | 204                                      |
| `POST /api/v1/workflows/{id}/run`  | `synthia-workflow`  | `workflows::run_workflow`                     | `WorkflowRun`                            |
| `GET /api/v1/evals`                | `synthia-eval`      | `evals::list_evals`                           | `List<EvalSuite>`                        |
| `GET /api/v1/evals/{name}`         | `synthia-eval`      | `evals::get_eval`                             | `EvalSuite`                              |
| `POST /api/v1/evals`               | `synthia-eval`      | `evals::create_eval`                          | body `EvalSuite` → 201                   |
| `DELETE /api/v1/evals/{name}`      | `synthia-eval`      | `evals::delete_eval`                          | 204                                      |
| `POST /api/v1/evals/{name}/run`    | `synthia-eval`      | `evals::run_eval`                             | `EvalReport`                             |
| `GET /api/v1/tasks`                | server-side scan    | `tasks::list_tasks`                           | `List<TaskDelegation>`                   |
| `GET /api/v1/tasks/{id}`           | server-side scan    | `tasks::get_task`                             | `TaskDelegation`                         |

Integration coverage: `crates/synthia-server/tests/management_routes_v2_test.rs`
— one round-trip test per domain (schedules CRUD, workflows
create/get/run/delete, evals create/run/delete, tasks empty-list).

## Persistence model

- **Schedules** — disk (`<workspace>/schedules/<id>.json`, PID-locked
  atomic writes) via the existing `synthia-scheduler::ScheduleStore`.
  Restart picks rows back up (`ScheduleStore::load` at boot).
- **Workflows** — in-memory `BTreeMap` behind a Tokio `RwLock`
  (`state/app_state/workflows.rs`). Restart clears; disk persistence
  mirrors the schedule-store pattern in a follow-up if needed.
- **Evals** — same in-memory shape (`state/app_state/evals.rs`).
- **Tasks** — no storage; derived on read from session JSONL
  (`Agent` envelopes), capped at the 50 most recent sessions.

## Deferred (follow-up turns)

1. **Real workflow execution** — `POST /workflows/{id}/run` validates
   the plan and returns a stubbed `WorkflowRun`: every `CallRun`
   settles as `superseded` carrying the error text
   `"WorkflowHost not yet wired into synthia-server — runs are
   stubbed"`. Real execution needs a `WorkflowHost` impl whose
   `spawn_agent` runs a full agent turn through the session
   controller machinery — a genuine R-series chunk, deliberately
   not faked with an echo host.
2. **Eval LLM-judge / schema-validation metrics** — the run endpoint
   scores via the keyword metric against a deterministic stub agent.
   Real `EvalAgent` wiring lands with the harness integration.
3. **Workflow structured form wizard** — the create modal accepts
   raw `WorkflowSpec` JSON (plan-validated server-side). The 5-variant
   structured picker (Agent / FanOut / BestOf / Pipeline / MCTS,
   recursive stages) is a follow-up UI turn.
4. **Workflow / eval disk persistence** — see above.
