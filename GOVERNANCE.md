# Governance

This document describes **how decisions are made** in Synthia today
and **how they will be made** once the project outgrows a single
maintainer. It is the single source of truth for governance; it
should be read together with [MAINTAINERS.md](MAINTAINERS.md) and
[CONTRIBUTING.md](CONTRIBUTING.md), which describe *who* the
maintainers are and *how* to participate.

This document is deliberately short. Most of the rules that actually
shape day-to-day work live in [AGENTS.md](AGENTS.md) — the
contribution contract — and in [`.github/CODEOWNERS`](.github/CODEOWNERS)
— the per-path review routing. Governance answers the questions
those don't.

## 1. Today: BDFL with explicit review surface

Synthia today is a **BDFL** project (Benevolent Dictator For
Life). The BDFL is [@crochee](https://github.com/crochee), the same
person listed in [MAINTAINERS.md](MAINTAINERS.md) and
[`.github/CODEOWNERS`](.github/CODEOWNERS).

| Decision type | Authority |
|---|---|
| Bug fixes, doc edits, test additions that pass CI | any contributor via PR; BDFL reviews |
| New feature in a single crate that doesn't touch public API | BDFL reviews; can be delegated to a `Reviewer:` comment |
| Change to a public API (`pub` surface, facade feature, provider wire type) | BDFL reviews **and** `make ci` is green |
| Change to a gate (`make check-*`), `Cargo.toml` workspace fields, `rust-toolchain.toml`, `.github/workflows/*` | BDFL reviews; these are policy changes, not code changes |
| Change to AGENTS.md / this document / license | BDFL + public rationale in PR description |
| Release tag | BDFL signs; the rest is `make ci` + the per-platform artifacts |

The BDFL role is **publicly reviewable**: every PR diff and every
release is on GitHub. Disagreement is resolved in the open, on the
PR or in [Discussions](https://github.com/crochee/synthia/discussions).

### Why BDFL, not a council, today

A maintainer council needs (a) ≥ 3 maintainers, (b) a written quorum
rule, and (c) a known conflict-resolution path. We don't have any
of those yet — adding ceremony before there are people to fill it
just slows the project down. The transition path is below.

## 2. The trigger to evolve

We will move from BDFL to a **maintainer council** when **any two**
of the following are true for two consecutive minor releases:

1. ≥ 3 active maintainers (≥ 5 distinct commits / reviews / issue
   responses per release).
2. ≥ 2 distinct organizations are running Synthia in production
   (one of them is the BDFL's own; the other is independent).
3. ≥ 1,000 GitHub stars **and** ≥ 50 active monthly contributors
   (PRs, reviews, issues, discussions).
4. A security issue required a coordinated multi-party disclosure
   in the last 6 months.

When the trigger fires, the BDFL opens a PR titled `governance:
transition BDFL → maintainer council` that:

- defines the **size** of the council (proposed: 3, max 5),
- defines the **decision rules** (proposed: simple-majority +
  BDFL veto for the first 2 cycles, then pure majority),
- defines the **quorum** (proposed: ≥ 2/3 of seated maintainers
  present, async votes accepted up to 14 days),
- defines **how maintainers are added** (proposed: any seated
  maintainer may nominate; vote by the rest; the nominee does not
  vote on themselves),
- defines **how maintainers are removed** (proposed: voluntary
  resignation, or 6 months of inactivity as defined in
  [MAINTAINERS.md](MAINTAINERS.md), or 2/3 vote of the rest).

The PR's body includes the trigger evidence (the two-of-four that
fired).

## 3. Conflict resolution

| Conflict | Path |
|---|---|
| Reviewer disagrees with the author on technical grounds | Discuss on the PR; the gate (`make ci`) is the impartial authority — if it passes and the reviewer still disagrees, the disagreement is recorded in the PR thread and the BDFL breaks the tie. |
| Author believes a reviewer is wrong | Reply on the PR with evidence; if no consensus in 7 days, escalate via a `@crochee` mention. |
| Two reviewers disagree | The BDFL breaks the tie. Disagreement is recorded in the PR thread (not buried in a private message). |
| Security issue that can't be discussed publicly | [SECURITY.md](SECURITY.md) — private disclosure. |
| Governance dispute (this document) | Open a PR titled `governance:` — proposals are decided in the open. |

## 4. Project-owned assets

| Asset | Owner |
|---|---|
| Repository | the synthia organization (or @crochee until an org exists) |
| Domain (`synthia.dev`, etc.) | the BDFL; transferred to the council once §2 fires |
| crates.io namespace | the same as above |
| Container registry | the same as above |
| Trademark / logo | the same as above |

A commercial deployment that ships a fork under its own name **must**
not reuse the Synthia name or logo for marketing the fork. The
project grants no trademark license under MIT.

## 5. Conflict-of-interest

The BDFL may have day-job affiliations with companies that consume
Synthia. The BDFL recuses from PRs that affect only that company
when the change is not generally beneficial. Recusal is noted in the
PR thread.

## 6. Changes to this document

A change to this document is a **governance change**, which is the
class of PR with the highest review bar:

1. PR opened against `master`.
2. Diff is only in this file (plus, optionally, MAINTAINERS.md and
   CODEOWNERS).
3. The PR title begins with `governance:`.
4. The PR body lists the trigger condition (§2) that justifies the
   change, OR explicitly states "this change does not move us
   toward the council, here is why".
5. After merging, [CHANGELOG.md `[Unreleased]`](CHANGELOG.md) gets a
   `Governance` bullet pointing at the merged PR.

A change that moves us **toward** the council must wait for §2's
trigger. A change that does not (e.g., clarifying language) does not.
