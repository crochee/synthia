# Entry Examples

Concrete examples of well-formatted entries.

## Learning Entry

```markdown
## [LRN-20250115-001] correction

**Logged**: 2025-01-15T10:30:00Z
**Priority**: high
**Status**: pending
**Area**: tests

### Summary
Incorrectly assumed pytest fixtures are scoped to function by default

### Details
When writing test fixtures, I assumed all fixtures were function-scoped. 
User corrected that while function scope is the default, the codebase 
convention uses module-scoped fixtures for database connections.

### Suggested Action
Check existing fixtures for scope patterns before defaulting to function scope.

### Metadata
- Source: user_feedback
- Related Files: tests/conftest.py
- Tags: pytest, testing, fixtures

---
```

## Learning: Resolved

```markdown
## [LRN-20250115-002] knowledge_gap

**Logged**: 2025-01-15T14:22:00Z
**Priority**: medium
**Status**: resolved
**Area**: config

### Summary
Project uses pnpm not npm for package management

### Details
Attempted to run `npm install` but project uses pnpm workspaces.
Lock file is `pnpm-lock.yaml`, not `package-lock.json`.

### Suggested Action
Check for `pnpm-lock.yaml` before assuming npm. Use `pnpm install`.

### Metadata
- Source: error
- Related Files: pnpm-lock.yaml
- Tags: package-manager, pnpm

### Resolution
- **Resolved**: 2025-01-15T14:30:00Z
- **Notes**: Added to CLAUDE.md for future reference

---
```

## Learning: Promoted

```markdown
## [LRN-20250115-003] best_practice

**Logged**: 2025-01-15T16:00:00Z
**Priority**: high
**Status**: promoted
**Promoted**: CLAUDE.md
**Area**: backend

### Summary
API responses must include correlation ID from request headers

### Details
All API responses should echo back the X-Correlation-ID header.
Required for distributed tracing.

### Suggested Action
Always include correlation ID passthrough in API handlers.

### Metadata
- Source: user_feedback
- Related Files: src/middleware/correlation.ts
- Tags: api, observability

---
```

## Error Entry

```markdown
## [ERR-20250115-A3F] docker_build

**Logged**: 2025-01-15T09:15:00Z
**Priority**: high
**Status**: pending
**Area**: infra

### Summary
Docker build fails on M1 Mac due to platform mismatch

### Error
```
error: failed to solve: python:3.11-slim: no match for platform linux/arm64
```

### Context
- Command: `docker build -t myapp .`
- Dockerfile uses `FROM python:3.11-slim`
- Running on Apple Silicon (M1/M2)

### Suggested Fix
Add platform flag: `docker build --platform linux/amd64 -t myapp .`

### Metadata
- Reproducible: yes
- Related Files: Dockerfile

---
```

## Error: Recurring Issue

```markdown
## [ERR-20250120-B2C] api_timeout

**Logged**: 2025-01-20T11:30:00Z
**Priority**: critical
**Status**: pending
**Area**: backend

### Summary
Third-party payment API timeout during checkout

### Error
```
TimeoutError: Request to payments.example.com timed out after 30000ms
```

### Context
- Command: POST /api/checkout
- Occurs during peak hours

### Suggested Fix
Implement retry with exponential backoff. Consider circuit breaker.

### Metadata
- Reproducible: yes (during peak hours)
- Related Files: src/services/payment.ts
- See Also: ERR-20250115-X1Y, ERR-20250118-Z3W

---
```

## Feature Request

```markdown
## [FEAT-20250115-001] export_to_csv

**Logged**: 2025-01-15T16:45:00Z
**Priority**: medium
**Status**: pending
**Area**: backend

### Requested Capability
Export analysis results to CSV format

### User Context
User runs weekly reports and needs to share results with non-technical 
stakeholders in Excel.

### Complexity Estimate
simple

### Suggested Implementation
Add `--output csv` flag to the analyze command.

### Metadata
- Frequency: recurring
- Related Features: analyze command

---
```

## Learning: Promoted to Skill

```markdown
## [LRN-20250118-001] best_practice

**Logged**: 2025-01-18T11:00:00Z
**Priority**: high
**Status**: promoted_to_skill
**Skill-Path**: skills/docker-m1-fixes
**Area**: infra

### Summary
Docker build fails on Apple Silicon due to platform mismatch

### Details
When building Docker images on M1/M2 Macs, the build fails because
the base image doesn't have an ARM64 variant.

### Suggested Action
Add `--platform linux/amd64` to docker build command.

### Metadata
- Source: error
- Related Files: Dockerfile
- Tags: docker, arm64, m1
- See Also: ERR-20250115-A3F, ERR-20250117-B2D

---
```

## Extracted Skill Example

When a learning is promoted to a skill:

**File**: `skills/docker-m1-fixes/SKILL.md`

```markdown
---
name: docker-m1-fixes
description: "Fixes Docker build failures on Apple Silicon (M1/M2). Use when docker build fails with platform mismatch errors."
---

# Docker M1 Fixes

Solutions for Docker build issues on Apple Silicon Macs.

## Quick Reference

| Error | Fix |
|-------|-----|
| `no match for platform linux/arm64` | Add `--platform linux/amd64` |

## Solutions

### Build Flag
```bash
docker build --platform linux/amd64 -t myapp .
```

### Dockerfile
```dockerfile
FROM --platform=linux/amd64 python:3.11-slim
```

## Source
- Learning ID: LRN-20250118-001
```
