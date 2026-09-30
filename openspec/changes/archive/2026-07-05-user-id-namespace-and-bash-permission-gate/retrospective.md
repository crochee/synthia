# Retrospective: user-id-namespace-and-bash-permission-gate

## What went well

- **Architecture clarity early**. The decision that "session is isolated by `session_id`, user mapping lives in `synthia-server`, and the agent has no `user` concept" kept `synthia-session` simple and avoided leaking authorization down the stack.
- **Defense in depth for bash**. Routing `BashTool` through `PermissionChecker` while keeping `CommandBlacklist` as an AND gate gives two independent fail-closed layers.
- **UTF-8 safe truncation as a shared helper**. Centralizing `cap_to_char_boundary` in `synthia-tool::builtin::utf8_safe` eliminated duplicate truncation logic and panic surfaces in both `web` and grep/web paths.
- **Server-layer isolation closure**. The legacy `/api/sessions/*` routes and the WebSocket endpoint now enforce ownership via `RequestUserId`, completing the user-isolation boundary.

## What was harder than expected

- **Plan assumed crates that do not exist**. §2 (`synthia-prompt`), §3 (`synthia-event`), and §4 (`synthia-event/src/log`) requirements had to be marked N/A because the underlying crates were never created. Future plans should validate crate existence before writing requirements.
- **Legacy migration wording**. The original delta spec claimed "automatic" legacy migration, but the implemented path is manual promotion via `SessionManager::assign_user`. The cumulative spec was updated to match reality.
- **Cumulative spec validation**. Initial sync included N/A requirements without scenarios, which failed `openspec validate --specs`. Removing the deferred requirements from the cumulative spec resolved the failure.

## Surprises

- **A previously archived change left drift in the cumulative spec**. `agent-tool-orchestrator-wiring/spec.md` still contained `## MODIFIED Requirements` headers, causing `check_synced_spec_format.sh` to fail. Fixing it here kept the CI gate green.

## Follow-up items

- `synthia-prompt` crate creation and `prompt_cache_key` HMAC injection (§2).
- `synthia-event` crate creation and `AgentEvent` version/seq fields (§3).
- `EventLogger` debounced flush with critical bypass (§4).
- Evaluate whether `SessionManager` internal HashMap should later move to `(user_id, session_id)` composite key for stricter in-memory isolation.
