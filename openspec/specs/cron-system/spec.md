## Purpose

Bridge the existing time-wheel scheduler and cron parser to agent task execution. Add `CronJobWrapper` with three execution modes (standalone / inject / session), `CronFileStore` for persistence, and `cron_add` / `cron_list` tools so users can trigger agent tasks on a schedule.

## Requirements

### Requirement: Cron jobs SHALL be wrapped for agent task execution
CronJobWrapper SHALL connect cron triggers to agent execution with three modes: standalone (execute independently, store result), inject (write message to session inbox, agent processes on next loop), and session (create new session for execution).

#### Scenario: Standalone cron job executes independently
- **WHEN** a cron job with mode "standalone" fires
- **THEN** the system SHALL execute the task directly and store the result

#### Scenario: Inject cron job writes to session inbox
- **WHEN** a cron job with mode "inject" fires and an active session exists
- **THEN** the system SHALL write a message to `~/.synthia/sessions/<session_id>/inbox/cron_job_<id>.md`

### Requirement: Cron job definitions SHALL be persisted to file
CronFileStore SHALL persist job definitions to `~/.synthia/cron_jobs.jsonl` in JSONL format (one JSON object per line). Each write operation SHALL use fsync() to ensure durability.

#### Scenario: Cron job is saved and survives restart
- **WHEN** a cron job is created via cron_add tool
- **THEN** the job definition SHALL be written to the JSONL file and SHALL be loaded on next startup

#### Scenario: CronFileStore handles corrupted lines gracefully
- **WHEN** the cron jobs file contains a malformed JSON line
- **THEN** the system SHALL skip the corrupted line, log a warning, and continue loading remaining jobs

### Requirement: Cron tools SHALL allow agent to manage scheduled tasks
The agent SHALL have access to cron_add (create scheduled task), cron_list (list all tasks), cron_remove (delete task), and cron_pause/cron_resume (pause/resume task) tools.

#### Scenario: Agent creates a cron job
- **WHEN** the agent calls cron_add with a cron expression and task description
- **THEN** the system SHALL create a new job with an auto-generated ID and return the job details

#### Scenario: Agent lists all cron jobs
- **WHEN** the agent calls cron_list
- **THEN** the system SHALL return a table of all jobs with ID, cron expression, task, mode, status, and next run time

### Requirement: Cron job frequency SHALL have a minimum interval
The minimum cron job interval SHALL be 1 minute. Attempts to schedule jobs with intervals shorter than 1 minute SHALL be rejected with an error.

#### Scenario: Sub-minute interval is rejected
- **WHEN** a user attempts to create a cron job with `* * * * * *` (every second)
- **THEN** the system SHALL reject the request with an error message

### Requirement: Mixed execution mode SHALL determine job mode automatically
When cron_add is called with mode "auto", the system SHALL determine the execution mode based on: standalone for pure information retrieval tasks; inject for reminder/notification tasks when an active session exists; session for complex tasks when no active session exists.

#### Scenario: Auto mode selects standalone for retrieval task
- **WHEN** cron_add is called with mode "auto" and task "查询今日天气"
- **THEN** the system SHALL set mode to "standalone"

#### Scenario: Auto mode selects inject for reminder
- **WHEN** cron_add is called with mode "auto" and task "提醒我吃药" with an active session
- **THEN** the system SHALL set mode to "inject"
