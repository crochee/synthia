# CLAUDE.md

Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

**Tradeoff:** These guidelines bias toward caution over speed. For trivial tasks, use judgment.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Note: After checking for applicable skills (Section 5), before responding with any solution:

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

## 5. Skills

**If a skill might apply, you must use it. No negotiation.**

- Subagent tasks: skip this rule
- User instructions (CLAUDE.md, GEMINI.md, AGENTS.md) take highest priority
- Otherwise, skill rules override default behavior

**Execution order:** receive task → check skills → respond (including clarifications)

1% rule: invoke skill to confirm if there's even 1% chance. Ignore if inapplicable; missing one cannot be recovered.

**Red Flags (you're rationalizing):**

| Thought | Reality |
|---------|---------|
| "Just a simple question" | Questions are tasks. Check skills. |
| "Need more context first" | Skill check precedes clarifications. |
| "Let me explore codebase first" | Skills tell you how to explore. |
| "I know this skill" | Invoke it, don't rely on memory. |
| "Just a small thing" | Check before doing anything. |
| "Skill is overkill" | Simple things become complex. |

**Priority:** Process skills (brainstorming, debugging) → Implementation skills

**Types:** Rigid skills (debugging, TDD) follow exactly; flexible skills adapt principles to context.

---

## 6. Task-Centric Execution

**Decompose with Task tool. Dispatch in parallel. Track with verification, not todos.**

### Core Rules

- **NEVER generate Plans** before starting work. Do not write markdown plans, task lists, or step-by-step outlines as part of your response. Planning is implicit in your reasoning.
- **ALWAYS use the Task tool** to decompose complex tasks into independent sub-tasks. The Task tool handles decomposition, context propagation, and result collection.
- **Dispatch independent tasks concurrently**. When two or more tasks have no shared state or sequential dependencies, dispatch them in parallel.
- **Do NOT use TodoWrite to track your own task progress**. The parent agent must not use TodoWrite for self-management. Sub-agents may use TodoWrite internally for their own decomposition, but the parent agent should track progress by awaiting sub-task results and verifying outcomes.

### Parallel Task Dispatch

When multiple tasks are independent:
1. Identify tasks with no shared state or sequential dependencies
2. Dispatch them concurrently using separate Task tool invocations
3. Wait for all results before proceeding to dependent work

Example of independent tasks that should be dispatched in parallel:
- Editing files in different modules with no cross-references
- Running unrelated test suites
- Searching different parts of the codebase for different patterns

Example of dependent tasks that must be sequential:
- Task B reads output from Task A
- Task B modifies a file that Task A will also modify
- Task B depends on a compilation artifact from Task A

### Context in Task Descriptions

When dispatching sub-tasks, include all necessary context in the task description:
- What files to modify and what changes to make
- Any constraints (e.g., "do not modify tests", "preserve existing API")
- Verification criteria (e.g., "run `cargo check` after changes", "ensure tests pass")
- Relevant code snippets if the context is small

### Verification Criteria

Every dispatched sub-task should specify how success is verified:
- "Make the change, then run `cargo test -p <crate>` to confirm"
- "Add the feature, then verify it compiles with `cargo check`"
- "Fix the bug, then confirm the test passes"

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.