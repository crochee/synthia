## ADDED Requirements

### Requirement: Cold memory search SHALL push filtering to SQL
The `search_with_mode` method SHALL NOT call `load_all_entries()` to fetch all entries. Instead, filtering and sorting SHALL be performed in the SQL query using WHERE, ORDER BY, and LIMIT clauses.

#### Scenario: Query with limit returns only needed entries
- **WHEN** ColdMemory::search_with_mode(query="test", limit=10, mode=BM25) is called
- **THEN** SQL query SHALL include "WHERE content LIKE '%test%' ORDER BY importance_score DESC LIMIT 10"

### Requirement: Cold memory SHALL use indexed queries for retrieval
All search operations SHALL use SQL queries that leverage indexes, avoiding full table scans of the cold_entries_fts virtual table.

#### Scenario: BM25 search optimization
- **WHEN** Search with BM25 ranking is requested
- **THEN** Query SHALL use FTS5 MATCH with ORDER BY bm25() and appropriate LIMIT

---

## MODIFIED Requirements

### Requirement: load_all_entries SHALL only be used for maintenance operations
The `load_all_entries()` method SHALL only be called internally for prune/flush operations. It SHALL NOT be called during normal search operations.

#### Scenario: Pruning still loads all entries
- **WHEN** ColdMemory::prune_context() is called
- **THEN** load_all_entries() MAY be called for the pruning logic

### Requirement: ColdEntry created_at SHALL be preserved from database
When loading cold entries, the created_at field SHALL be read from the database and NOT replaced with Utc::now().

#### Scenario: Entry loaded from database
- **WHEN** ColdEntry is fetched from cold_entries_meta table
- **THEN** The created_at field SHALL match the stored RFC3339 timestamp

---

## REMOVED Requirements

### Requirement: (None - no requirements being removed)

---

## RENAMED Requirements

### Requirement: (None - no requirements being renamed)