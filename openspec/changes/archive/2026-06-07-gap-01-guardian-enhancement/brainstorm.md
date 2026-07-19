<!--
Raw capture of superpowers:brainstorming output.

本檔原樣捕捉 brainstorming skill 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# GAP-01 Guardian Enhancement - Brainstorming Decision Log

## Background
Gap analysis identified Synthia's Guardian implementation vs opencode's production-grade Guardian.
Current Synthia issues:
- SimpleGuardian: hardcoded `rm`/`sudo` detection only, no session isolation
- GuardianReviewer: single LLM call, no timeout, no transcript compression
- CircuitBreaker: only tracks compaction failures, NOT Guardian rejections

## Decision Chain

### Q1: Architecture
- **Option A**: Centralized Guardian (opencode style)
- **Option B**: Per-Agent Guardian (delegates)
- **Option C**: Hybrid (recommended) — Local Guardian for fast-path, escalation to centralized for complex cases

**Decision**: C — Hybrid approach aligns with Synthia's existing hook system.

### Q2: LLM Session Handling
- **Option A**: Single-shot LLM call (current Synthia)
- **Option B**: Timeout with fallback (30s)
- **Option C**: Compressed transcript
- **Option D**: Multi-turn review

**Recommendation**: B + C combined — Timeout 30s + compressed transcript + fail-closed.

### Q3: Circuit Breaker Strategy
opencode tracks: 3 consecutive denials OR 10 total denials triggers session interrupt.

**Decision**: Adopt opencode strategy directly.

### Q4: User Confirmation Mode
- **Option A**: Blocking confirm
- **Option B**: Non-blocking prompt
- **Option C**: Interrupt + resume
- **Option D**: Configurable per action type

**Decision**: D — Different action types need different strategies:
- Shell/Exec: blocking
- Network: non-blocking
- Credential: interrupt

### Q5: Guardian + Hook Integration
- **Option A**: Guardian as upper-layer coordinator (recommended)
- **Option B**: Independent safety layer
- **Option C**: Hook calls Guardian

**Decision**: A — Guardian as upper-layer coordinator, hook system calls Guardian.check().

### Q6: Fail-closed Strategy
- **Option A**: Fail-open
- **Option B**: Fail-closed
- **Option C**: Degrade to SimpleGuardian

**Recommendation**: Fail-closed + Degrade to SimpleGuardian combined.

## Design Trade-offs Resolved

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Architecture | C: Hybrid | Local fast-path + centralized escalation |
| LLM Session | B+C: Timeout + compressed | 30s timeout, fail-closed |
| Circuit Breaker | 3 consecutive / 10 total | Matches opencode pattern |
| Confirmation Mode | D: Per action type | Shell=blocking, Network=async, Credential=interrupt |
| Hook Integration | A: Coordinator | Guardian sees all operations via hooks |
| Fail-closed | Degrade | LLM fail → SimpleGuardian, service fail → deny |

## Validated Design Summary

1. **Hybrid Guardian Layer**: SimpleGuardian (rule-based) + GuardianReviewer (LLM-based)
2. **30s LLM timeout with compression**: fail-closed on timeout
3. **Circuit breaker**: 3 consecutive or 10 total denials → session interrupt
4. **Action-type confirmation**: Shell=blocking, Network=async, Credential=interrupt
5. **Hook integration**: Hook calls Guardian.check(), Guardian decides Allow/Deny/NeedUserConfirm
6. **Graceful degradation**: LLM failure → SimpleGuardian fallback