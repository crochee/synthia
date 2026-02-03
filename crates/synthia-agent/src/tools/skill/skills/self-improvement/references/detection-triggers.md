# Detection Triggers

Conditions that should trigger learning capture and priority/area guidelines.

## Detection Triggers

Automatically log when you notice:

### Corrections (→ learning with `correction` category)
- "No, that's not right..."
- "Actually, it should be..."
- "You're wrong about..."
- "That's outdated..."

### Feature Requests (→ feature request)
- "Can you also..."
- "I wish you could..."
- "Is there a way to..."
- "Why can't you..."

### Knowledge Gaps (→ learning with `knowledge_gap` category)
- User provides information you didn't know
- Documentation you referenced is outdated
- API behavior differs from your understanding

### Errors (→ error entry)
- Command returns non-zero exit code
- Exception or stack trace
- Unexpected output or behavior
- Timeout or connection failure

### Best Practices (→ learning with `best_practice` category)
- Found better approach for a recurring task
- Discovered non-obvious solution through investigation
- Identified pattern that prevents issues

## Priority Guidelines

| Priority | When to Use |
|----------|-------------|
| `critical` | Blocks core functionality, data loss risk, security issue |
| `high` | Significant impact, affects common workflows, recurring issue |
| `medium` | Moderate impact, workaround exists |
| `low` | Minor inconvenience, edge case, nice-to-have |

## Area Tags

Use to filter learnings by codebase region:

| Area | Scope |
|------|-------|
| `frontend` | UI, components, client-side code |
| `backend` | API, services, server-side code |
| `infra` | CI/CD, deployment, Docker, cloud |
| `tests` | Test files, testing utilities, coverage |
| `docs` | Documentation, comments, READMEs |
| `config` | Configuration files, environment, settings |

## Best Practices

1. **Log immediately** - context is freshest right after the issue
2. **Be specific** - future agents need to understand quickly
3. **Include reproduction steps** - especially for errors
4. **Link related files** - makes fixes easier
5. **Suggest concrete fixes** - not just "investigate"
6. **Use consistent categories** - enables filtering
7. **Promote aggressively** - if in doubt, add to CLAUDE.md or .github/copilot-instructions.md
8. **Review regularly** - stale learnings lose value
