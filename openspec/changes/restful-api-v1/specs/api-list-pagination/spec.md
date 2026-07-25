## ADDED Requirements

### Requirement: List generic type
The system SHALL provide a generic `List<T>` type for all list endpoints with fields: `data: Vec<T>`, `next_cursor: Option<String>`, `total: Option<u64>`.

#### Scenario: List with more data
- **WHEN** a list query returns items and there are more items beyond the current page
- **THEN** the response SHALL contain `data` with the items, `next_cursor` as base64 of the last item's ID, and `total` as the total count

#### Scenario: List at end
- **WHEN** a list query returns the final page of items
- **THEN** the response SHALL contain `data` with the items and `next_cursor` as `null`; `total` SHALL be present if computable

#### Scenario: Large table total omission
- **WHEN** a list query targets a large dataset where COUNT is expensive
- **THEN** the response MAY omit `total` (field not present in JSON)

---

### Requirement: PageQuery parameter type
The system SHALL provide a `PageQuery` type with fields: `cursor: Option<String>`, `limit: Option<u64>`, `sort: Option<String>`. Default limit SHALL be 20, maximum limit SHALL be 100.

#### Scenario: First page request
- **WHEN** a list endpoint is called without `cursor`
- **THEN** the system SHALL return items from the beginning of the sorted result

#### Scenario: Subsequent page request
- **WHEN** a list endpoint is called with `cursor` from a previous response's `next_cursor`
- **THEN** the system SHALL return items after the resource identified by the decoded cursor

#### Scenario: Limit boundary — zero
- **WHEN** a list endpoint is called with `limit=0`
- **THEN** the system SHALL return HTTP 400 with error code `bad_request`

#### Scenario: Limit boundary — exceeds maximum
- **WHEN** a list endpoint is called with `limit` greater than 100
- **THEN** the system SHALL silently truncate limit to 100

#### Scenario: Limit boundary — default
- **WHEN** a list endpoint is called without `limit`
- **THEN** the system SHALL use default limit of 20

---

### Requirement: Cursor encoding
The cursor SHALL be the base64 (URL-safe, no-padding) encoding of the last resource's ID on the current page. Clients SHALL treat cursor as opaque.

#### Scenario: Cursor encodes resource ID
- **WHEN** the last item on a page has id "task_abc"
- **THEN** `next_cursor` SHALL be `base64("task_abc")` = `"dGFza19hYmM="`

#### Scenario: Cursor decode failure
- **WHEN** a client sends an invalid cursor that cannot be base64-decoded
- **THEN** the system SHALL return HTTP 400 with error code `invalid_cursor`

#### Scenario: Cursor points to deleted resource
- **WHEN** a client sends a cursor whose decoded ID no longer exists
- **THEN** the system SHALL return `{ "data": [], "next_cursor": null }`

---

### Requirement: Sort whitelist
Each resource type SHALL define a whitelist of allowed sort fields. The `sort` parameter uses field name with `-` prefix for descending order.

#### Scenario: Valid sort field
- **WHEN** a list endpoint is called with `sort=-created_at` and `created_at` is in the resource's whitelist
- **THEN** the system SHALL return items sorted by created_at descending

#### Scenario: Invalid sort field
- **WHEN** a list endpoint is called with `sort=invalid_field`
- **THEN** the system SHALL return HTTP 400 with error code `invalid_sort_field`

#### Scenario: No sort specified
- **WHEN** a list endpoint is called without `sort`
- **THEN** the system SHALL use the resource's default sort (`-created_at` for most resources, `score` DESC for memory/search)

---

### Requirement: Resource-specific sort whitelists
The system SHALL define the following sort whitelists: tasks (created_at, updated_at, status), skills (name, created_at), tools (name), providers (name), commands (name), jobs (key, trigger_desc), mcp/servers (name), approvals (created_at, status).

#### Scenario: Tasks sort by status
- **WHEN** `GET /api/v1/tasks?sort=status`
- **THEN** the system SHALL return tasks sorted by status ascending

#### Scenario: Memory search ignores custom sort
- **WHEN** `GET /api/v1/memory/search?q=kw&sort=name`
- **THEN** the system SHALL ignore the sort parameter and use fixed `score` DESC order

---

### Requirement: Resource-specific query filters
TaskPageQuery SHALL extend PageQuery with `status: Option<String>` and `context_id: Option<String>`. JobPageQuery SHALL extend PageQuery with `key: Option<String>` and `trigger_contains: Option<String>`.

#### Scenario: Filter tasks by status
- **WHEN** `GET /api/v1/tasks?status=working`
- **THEN** the system SHALL return only tasks with A2A TaskState `working`

#### Scenario: Invalid task status filter
- **WHEN** `GET /api/v1/tasks?status=nonexistent`
- **THEN** the system SHALL return HTTP 400 with error code `bad_request`

#### Scenario: Filter jobs by key
- **WHEN** `GET /api/v1/jobs?key=cleanup`
- **THEN** the system SHALL return only jobs whose key contains "cleanup"
