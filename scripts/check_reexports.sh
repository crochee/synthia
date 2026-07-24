#!/usr/bin/env bash
# scripts/check_reexports.sh
#
# Layer-3 defense for the synthia_session re-export policy.
#
# This is the OUT-OF-BAND companion to the compile-fail doc tests in
# src/lib.rs and the integration test in tests/reexport_policy.rs. Those
# tests prove the FORBIDDEN paths don't compile. This script proves the
# STRUCTURE of the re-export block in src/lib.rs has not been corrupted
# by an inattentive edit (e.g. someone re-adding
#   `pub use session::{Session, SessionError, SessionManager}`
# which would re-introduce the name-shadowing trap fixed on 2026-06-13).
#
# This script is intentionally lightweight (pure grep + awk) so it can
# run in any CI environment without external tooling. It is NOT a
# replacement for the cargo tests; it is a fast pre-check that runs in
# under 1 second.
#
# Usage:
#   ./scripts/check_reexports.sh                    # check current tree
#   ./scripts/check_reexports.sh path/to/lib.rs    # check a specific file
#
# Exit codes:
#   0  - all checks pass
#   1  - at least one check failed (see error message)
#   2  - lib.rs not found
#
# To add a new check:
#   1. Add the rule below with a comment block explaining the invariant.
#   2. Update the doc test in src/lib.rs (Layer 1) and the integration
#      test in tests/reexport_policy.rs (Layer 2) to mirror the same
#      invariant. The three layers MUST be kept in sync.

set -euo pipefail

# ---------------------------------------------------------------------------
# Locate lib.rs
# ---------------------------------------------------------------------------
LIB_RS="${1:-crates/synthia-session/src/lib.rs}"

if [[ ! -f "$LIB_RS" ]]; then
    echo "ERROR: $LIB_RS not found." >&2
    echo "  Pass the path to the synthia-session lib.rs as the first arg," >&2
    echo "  or run this script from the workspace root." >&2
    exit 2
fi

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
fail() {
    echo "RE-EXPORT POLICY VIOLATION: $1" >&2
    echo "  See $LIB_RS for the policy block, or the historical fix" >&2
    echo "  in tests/reexport_policy.rs for the runtime invariant." >&2
    EXIT_CODE=1
}

EXIT_CODE=0

# Extract all `pub use ...;` lines from the active code (skip doc
# comments, line comments, and string literals).
extract_pub_use_lines() {
    awk '
        # Skip doc comments and line comments
        /^[[:space:]]*(\/\/|\/\*)/ { next }
        # Skip if we are inside a multi-line block comment
        in_block { if (/\*\//) in_block=0; next }
        /\/\*/ && !/\*\// { in_block=1; next }
        # Match `pub use ...;` lines
        /pub[[:space:]]+use[[:space:]]+/ {
            print FILENAME ":" NR ": " $0
        }
    ' "$LIB_RS"
}

# ---------------------------------------------------------------------------
# Check 1: The historical offender MUST NOT appear as active code.
# ---------------------------------------------------------------------------
# The line that caused the 2026-06-13 name-shadowing bug was
#     pub use session::{Session, SessionError, SessionManager};
# We forbid any active (un-commented) line of the form
#     pub use ... session::{Session, SessionError, SessionManager}
# because the trailing three names are exactly the ones that shadow.
echo 'Check 1: historical offender "pub use session::{...SessionManager...}" absent...'
while IFS= read -r line; do
    if echo "$line" | grep -q "pub use.*session::.*SessionManager"; then
        fail "Found active 'pub use ... session::... SessionManager ...' line:
  $line"
    fi
done < <(extract_pub_use_lines)

# ---------------------------------------------------------------------------
# Check 2: The re-export block MUST reference all expected modules.
# ---------------------------------------------------------------------------
# The current policy keeps a small explicit re-export list. Each of
# these modules MUST be re-exported (or globs) in lib.rs. If any are
# missing, the public API has drifted.
echo "Check 2: required module re-exports present..."
for module in "manager::" "service::" "state_machine::" "store::" "token_budget::" "types::*"; do
    if ! grep -qE "^[[:space:]]*pub[[:space:]]+use[[:space:]]+.*${module//\*/\\*}" "$LIB_RS"; then
        fail "Missing required re-export for module pattern: $module"
    fi
done

# ---------------------------------------------------------------------------
# Check 3: The policy header MUST be present and reference the 3
# known-conflict names.
# ---------------------------------------------------------------------------
echo "Check 3: policy header documents the 3 conflict names..."
for name in "Session" "SessionManager" "SessionError"; do
    if ! grep -q "$name" "$LIB_RS"; then
        fail "Policy header must mention $name"
    fi
done
if ! grep -q "Re-export policy (synthia-session)" "$LIB_RS"; then
    fail "Policy header section 'Re-export policy (synthia-session)' not found"
fi

# ---------------------------------------------------------------------------
# Check 4: The integration test MUST exist and reference the 3 layers.
# ---------------------------------------------------------------------------
echo "Check 4: integration test 'reexport_policy.rs' exists..."
REEEXPORT_TEST="$(dirname "$LIB_RS")/../tests/reexport_policy.rs"
if [[ ! -f "$REEEXPORT_TEST" ]]; then
    fail "Integration test not found: $REEEXPORT_TEST"
else
    if ! grep -q "Layer 1" "$REEEXPORT_TEST" || \
       ! grep -q "Layer 2" "$REEEXPORT_TEST" || \
       ! grep -q "Layer 3" "$REEEXPORT_TEST"; then
        fail "Integration test must mention all 3 layers (Layer 1 / 2 / 3) in its header"
    fi
fi

# ---------------------------------------------------------------------------
# Check 5: Consumers MUST use qualified paths for the 3 multi-ownership
# types. We grep the workspace for the forbidden short patterns.
# ---------------------------------------------------------------------------
echo "Check 5: workspace uses qualified paths for multi-ownership types..."
WORKSPACE_ROOT="$(cd "$(dirname "$LIB_RS")/../.." && pwd)"

# Allowed sites for the short `synthia_session::SessionManager` form:
#   - this lib.rs itself (in the policy comment / doc tests)
#   - the tests/reexport_policy.rs file (the Layer 2 test deliberately
#     uses both forms)
#   - any *_reexport_*.rs fixture
#   - any file under target/ (build output)
#   - docs/ (design notes may quote the forbidden patterns)
SHORT_HITS="$(grep -rEn \
    '(synthia_session::SessionManager|synthia_session::SessionError)' \
    --include='*.rs' \
    --exclude-dir=target \
    --exclude-dir=.git \
    --exclude-dir=node_modules \
    --exclude='reexport_policy.rs' \
    --exclude='lib.rs' \
    "$WORKSPACE_ROOT" || true)"

if [[ -n "$SHORT_HITS" ]]; then
    fail "Found unqualified \`synthia_session::SessionManager\` or \`synthia_session::SessionError\`:
$SHORT_HITS

Use the qualified paths instead:
  synthia_session::manager::SessionManager   (struct)
  synthia_session::session::SessionManager    (trait)
  synthia_session::session::SessionError"
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
if [[ $EXIT_CODE -eq 0 ]]; then
    echo "OK: all 5 re-export policy checks passed."
fi
exit $EXIT_CODE
