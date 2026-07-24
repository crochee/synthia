#!/usr/bin/env bash
# scripts/check_synced_spec_format.sh
#
# CI gate: enforce cumulative format for OpenSpec synced spec files.
#
# OpenSpec requires files under `openspec/specs/*/spec.md` (the
# CUMULATIVE spec path) to use the bare `## Requirements` header. The
# delta-style headers `## ADDED Requirements` / `## MODIFIED Requirements`
# are only valid inside `openspec/changes/<name>/specs/<capability>/spec.md`
# (the DELTA spec path). Using delta headers in the synced path causes
# `openspec spec validate --strict` to fail with:
#   "Spec must have a Requirements section. Missing required sections.
#    Expected headers: '## Purpose' and '## Requirements'."
#
# This script is intentionally lightweight (pure grep + find) so it can
# run in any CI environment without external tooling. It is NOT a
# replacement for `openspec spec validate --strict`; it is a fast
# pre-check that runs in under 1 second and produces actionable output
# (file path of the offender).
#
# Usage:
#   ./scripts/check_synced_spec_format.sh
#
# Exit codes:
#   0  - all synced specs use cumulative format
#   1  - at least one synced spec uses delta format (see error message)
#
# History: 12 pre-existing spec files drifted into delta format before
# the 2026-06-14 fix. This script prevents future drift.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SYNCED_SPECS_DIR="${REPO_ROOT}/openspec/specs"
DRIFT_PATTERN='^## (ADDED|MODIFIED) Requirements$'

if [ ! -d "${SYNCED_SPECS_DIR}" ]; then
    echo "ERROR: synced specs directory not found: ${SYNCED_SPECS_DIR}" >&2
    exit 1
fi

drift_files=$(grep -rlE "${DRIFT_PATTERN}" "${SYNCED_SPECS_DIR}" 2>/dev/null || true)

if [ -n "${drift_files}" ]; then
    echo "FAIL: synced spec format drift detected." >&2
    echo "" >&2
    echo "The following files use delta-style headers but live in the" >&2
    echo "cumulative path (openspec/specs/). They MUST use bare" >&2
    echo "'## Requirements' instead of '## ADDED Requirements' or" >&2
    echo "'## MODIFIED Requirements':" >&2
    echo "" >&2
    echo "${drift_files}" | sed 's/^/  - /' >&2
    echo "" >&2
    echo "Fix: rename the header to '## Requirements' (one-line sed per file)." >&2
    exit 1
fi

total_specs=$(find "${SYNCED_SPECS_DIR}" -name "spec.md" -type f | wc -l)
echo "OK: ${total_specs} synced specs are in cumulative format (no delta headers)."
exit 0
