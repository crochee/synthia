#!/usr/bin/env bash
# Harness shape gate: no `synthia-harness` production function may exceed the
# length / nesting budget in `scripts/harness-shape/clippy.toml`.
#
# Why this exists (R122): `clippy.toml` has always set
# `cognitive-complexity-threshold = 20`, but `clippy::cognitive_complexity`
# is allow-by-default and — per its own documentation — no longer measures
# what its name suggests: it charges decision points only, with no penalty
# for nesting depth, loops, `?`, or closure nesting. Measured against the
# algorithm clippy 1.98 ships, the worst function in `agent::{re_act,
# strategy}` scored 8/20, so that threshold enforced nothing. The properties
# this codebase actually holds are **flat nesting** and **short functions**;
# these two lints are what clippy's docs recommend in place of the inert one.
#
# Scoped to `synthia-harness`'s production code on purpose: the rest of the
# workspace has long functions this round did not touch, and making those
# hard errors would be an unrequested refactor. The scoping is why the
# thresholds live in their own `clippy.toml` (see that file) rather than the
# workspace one — setting them at the root *enables* both allow-by-default
# lints everywhere, and `make lint-rust` runs with `-D warnings`.
#
# Exit 0 with an `OK:` line, or exit 1 with the offending sites.
set -euo pipefail

cd "$(dirname "$0")/.."

conf_dir="scripts/harness-shape"
conf_file="$conf_dir/clippy.toml"
if [[ ! -f "$conf_file" ]]; then
  echo "FAIL: $conf_file not found — the shape budget has no source of truth" >&2
  exit 1
fi

line_limit=$(awk -F' = ' '/^too-many-lines-threshold/ { print $2 }' "$conf_file")
nest_limit=$(awk -F' = ' '/^excessive-nesting-threshold/ { print $2 }' "$conf_file")
if [[ -z "$line_limit" || -z "$nest_limit" ]]; then
  echo "FAIL: $conf_file must set too-many-lines-threshold and excessive-nesting-threshold" >&2
  exit 1
fi

# `excessive_nesting` is disabled at its default value (0), so a 0 here
# would make half this gate a no-op that still prints OK.
if [[ "$nest_limit" -eq 0 ]]; then
  echo "FAIL: excessive-nesting-threshold = 0 disables the lint; the nesting budget would be unenforced" >&2
  exit 1
fi

# cargo does NOT fingerprint clippy.toml, so a threshold change alone would
# replay a cached verdict. Touching the crate root forces a real re-lint.
touch crates/synthia-harness/src/lib.rs

report=$(mktemp)
trap 'rm -f "$report"' EXIT

set +e
CLIPPY_CONF_DIR="$conf_dir" \
  cargo clippy -p synthia-harness --lib --all-features -- \
  -W clippy::too_many_lines \
  -W clippy::excessive_nesting \
  >"$report" 2>&1
status=$?
set -e

# Every lint here is enabled with `-W` (warn), so warnings never fail the
# build: a non-zero status means the crate did not compile (or clippy itself
# failed). Failing loudly matters — with no diagnostics there would be
# nothing to report, and a shape-only check would pass vacuously on a broken
# tree.
if [[ $status -ne 0 ]]; then
  echo "FAIL: 'cargo clippy -p synthia-harness --lib' exited $status" >&2
  echo "      the shape budget cannot be evaluated on a tree that does not build:" >&2
  grep -E '^error(\[|:)' "$report" | head -5 >&2 || true
  exit 1
fi

# clippy's own wording: "this block is too nested" / "this function has too
# many lines". Match the message, then keep only the location lines that
# point into the crate's production sources.
sites=$(grep -A3 -E 'is too nested|has too many lines' "$report" \
  | grep -E '^\s+--> crates/synthia-harness/src/' || true)

if [[ -n "$sites" ]]; then
  echo "FAIL: synthia-harness production code broke the harness shape budget"
  echo "      (max $line_limit lines, max $nest_limit nesting levels)"
  echo "$sites"
  echo "Extract a named helper — each location above names the function."
  exit 1
fi

echo "OK: no synthia-harness production fn exceeds $line_limit lines or nesting $nest_limit"
