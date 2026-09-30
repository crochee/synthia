## ADDED Requirements

### Requirement: AgentPath Hierarchical Addressing
The system SHALL define `AgentPath` as a validated absolute path starting with `/root`. Path segments MUST match `[a-z0-9][a-z0-9_-]{0,63}`. Invalid path formats SHALL be rejected at validation. The path MUST be lowercase letters, digits, underscores, and hyphens only.

### Requirement: AgentRegistry Resource Management
The system SHALL maintain `AgentRegistry` as a shared registry keyed by `AgentPath`. The registry SHALL track active agents with `AgentMetadata { agent_id, agent_path, agent_nickname, agent_role, last_task_message }`. The registry SHALL enforce `max_concurrent` (16) using atomic `compare_exchange_weak` and `max_depth` (4).

### Requirement: AgentControl Shared Handle
The system SHALL provide `AgentControl` as a lightweight cloneable handle (`Arc<AgentRegistry> + Weak<ThreadManagerState>`). AgentControl SHALL support `spawn_agent`, `send_message`, `list_agents`, `shutdown_agent_tree` operations. The weak reference pattern SHALL prevent reference cycles.

### Requirement: SpawnReservation RAII Pattern
The system SHALL implement `SpawnReservation` with two-phase commit: reserve (allocates spawn slot, locks nickname) and commit (finalizes thread creation). On drop without commit, the reservation SHALL automatically release the slot and nickname.

### Requirement: Mailbox Communication
The system SHALL implement per-session `Mailbox` with MPSC channel and `watch::Sender<u64>` sequence counter. The mailbox SHALL support `send()` for inter-agent messages. Mailbox messages SHALL NOT be permission-checked at send time — permission checks occur at tool execution time.

### Requirement: MailboxDeliveryPhase State Machine
The system SHALL implement `MailboxDeliveryPhase` with three states: `CurrentTurn` (messages join current model request), `NextTurn` (messages queue for next turn), `Suspended` (Ask阻塞期间，messages accumulate in pending buffer). The phase SHALL transition from `Suspended` to `NextTurn` when user resolves the pending Ask.

### Requirement: CompletionWatcher
The system SHALL spawn a detached `tokio::spawn` task to monitor sub-agent completion. The watcher SHALL send `InterAgentCommunication` to parent on terminal status. Parent MAY fire-and-forget this watcher.

---

## MODIFIED Requirements

None — this is a new capability.

---

## REMOVED Requirements

None — this is a new capability.

---

## RENAMED Requirements

None.