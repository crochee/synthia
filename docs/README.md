# docs/

This is the **live documentation tree** for Synthia. The two files
you almost certainly want first are:

- [`INDEX.md`](INDEX.md) — the live doc tree (every active document,
  grouped by purpose).
- [`ARCHIVE.md`](ARCHIVE.md) — the frozen record of historical
  optimization reports and superseded specs (linked **only** from
  here; live docs cite only what is current).

If you arrived from a deep link, the table below is the shortest
path back to a current entry point:

| I want… | Open |
| :--- | :--- |
| 5-minute hello-world | [`QUICKSTART.md`](QUICKSTART.md) |
| Crate / trait map | [`INDEX.md`](INDEX.md) §1 and §5 (ADR) |
| The 4-step MVP guide | [`../MINIMAL.md`](../MINIMAL.md) |
| The 7 substitutable components | [`../SEAMS.md`](../SEAMS.md) |
| Every runnable example | [`examples/README.md`](examples/README.md) |
| Current round's open tail | [`../CHANGELOG.md`](../CHANGELOG.md) `[Unreleased]` |
| Frozen historical record | [`ARCHIVE.md`](ARCHIVE.md) |

## Subdirectories

| Path | Contents |
| :--- | :--- |
| `architecture/` | ADR (accepted records), `review/` (frozen review notes) |
| `design/` | One-off design notes (e.g. `web-ux-optimization.md`) |
| `examples/` | Every executable example with proof line |
| `interface-contract/` | The cross-crate wire / schema contract |
| `superpowers/specs/` | **Active** specs (≥ 2026-09-19) |
| `superpowers/decisions/` | Cross-cutting decisions |
| `superpowers/plans/` | Implementation plans paired with their spec |

Historical optimization reports (`optimization-report-R*.md`) live at
this directory's root and are catalogued in [`ARCHIVE.md`](ARCHIVE.md).