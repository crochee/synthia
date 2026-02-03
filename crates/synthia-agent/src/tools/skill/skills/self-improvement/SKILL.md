---
name: self-improvement
description: "Captures learnings, errors, and corrections for continuous improvement. Use when: (1) A command or operation fails, (2) User corrects you, (3) You discover non-obvious solutions, (4) You identify knowledge gaps. Log to .learnings/ directory and promote high-value learnings to project memory."
---

# Self-Improvement Skill

Log learnings and errors to markdown files for continuous improvement. Important learnings can be promoted to project memory files.

## Quick Reference

| Situation | Action |
|-----------|--------|
| Command/operation fails | Log to `.learnings/ERRORS.md` |
| User corrects you | Log to `.learnings/LEARNINGS.md` with category `correction` |
| User wants missing feature | Log to `.learnings/FEATURE_REQUESTS.md` |
| Knowledge was outdated | Log to `.learnings/LEARNINGS.md` with category `knowledge_gap` |
| Found better approach | Log to `.learnings/LEARNINGS.md` with category `best_practice` |
| Broadly applicable learning | Promote to `CLAUDE.md` or `AGENTS.md` |

## Core Workflow

1. **Detect** - Notice errors, corrections, knowledge gaps, or better approaches
2. **Log** - Create entry in appropriate `.learnings/` file with proper format
3. **Review** - Periodically check pending items and link related entries
4. **Promote** - Move high-value learnings to project memory files
5. **Resolve** - Mark entries as resolved when fixed

## Learning Files

Create `.learnings/` directory in your project:

```bash
mkdir -p .learnings
```

### File Structure

```
.learnings/
├── LEARNINGS.md         # Corrections, insights, knowledge gaps, best practices
├── ERRORS.md            # Command failures, exceptions
└── FEATURE_REQUESTS.md  # User-requested capabilities
```

### Entry Format

Each entry follows a consistent format with ID, metadata, and content sections.

**See [logging-format.md](references/logging-format.md) for detailed format specifications.**

## Promotion

When a learning is broadly applicable, promote it to project memory:

| Target | Content |
|--------|---------|
| `CLAUDE.md` | Project facts, conventions, gotchas |
| `AGENTS.md` | Workflows, tool patterns, automation rules |
| `.github/copilot-instructions.md` | Project context for GitHub Copilot |

### Promotion Process

1. Distill learning into concise rule
2. Add to appropriate target file
3. Update original entry status to `promoted`

**See [promotion-guide.md](references/promotion-guide.md) for detailed promotion guidelines.**

## Detection Triggers

Log when you notice:
- **Corrections**: User says "No, that's wrong..." or "Actually..."
- **Feature Requests**: User asks "Can you also..." or "I wish you could..."
- **Knowledge Gaps**: User provides info you didn't know
- **Errors**: Command fails, exception occurs, unexpected behavior

**See [detection-triggers.md](references/detection-triggers.md) for complete trigger list and priority guidelines.**

## Skill Extraction

When a learning is valuable enough to become a reusable skill:

### Extraction Criteria

- Has `See Also` links to 2+ similar issues (recurring)
- Status is `resolved` with working fix (verified)
- Required debugging to discover (non-obvious)
- Useful across codebases (broadly applicable)
- User says "save this as a skill"

### Extraction Workflow

1. Create `skills/<skill-name>/SKILL.md` using template
2. Fill in template with learning content
3. Update learning entry with `promoted_to_skill` status
4. Test skill in fresh session

**See [examples.md](references/examples.md) for entry examples and extracted skill examples.**

## Templates

- [LEARNINGS.md template](assets/LEARNINGS.md) - Template for learning file header
- [SKILL-TEMPLATE.md](assets/SKILL-TEMPLATE.md) - Template for extracting skills

## References

- [logging-format.md](references/logging-format.md) - Detailed entry formats
- [promotion-guide.md](references/promotion-guide.md) - Promotion guidelines
- [detection-triggers.md](references/detection-triggers.md) - Trigger conditions
- [examples.md](references/examples.md) - Concrete entry examples
