# Trait-Abstraction-Review Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a comprehensive research review of all 56 `pub trait` definitions in the Synthia workspace, classify each as KEEP/REVIEW/REMOVE_CANDIDATE, and write deep-reviews for high-signal traits — without modifying any `src/` business code.

**Architecture:** Hybrid research methodology. Phase A uses a 0-dependency bash + `rg` + `awk` script to extract 8 quantitative signals per trait into `artifacts/trait-inventory.md`. Phase C uses a structured template with 4-party adversarial review (怀疑派/架构派/生产派/简化派) to deeply review 10-15 high-signal traits. Synthesis phase produces a 3-bucket classification and a future-refactor candidate index. All output is contained in the OpenSpec change directory.

**Tech Stack:** bash 4+, ripgrep (rg), awk, openspec CLI 1.3.1

**Important:** All files created by this plan live under `openspec/changes/2026-06-15-trait-abstraction-review/`. Per project policy (`.gitignore` line 37: `openspec`), these files are **local-only** and not committed to git. Progress is tracked by file existence, not git commits. This plan was originally written with commit steps; those have been removed in v2.

---

## File Structure

This plan creates the following files within `openspec/changes/2026-06-15-trait-abstraction-review/`:

| File | Purpose | Phase |
|------|---------|-------|
| `scripts/extract_trait_signals.sh` | 8-signal extractor (0 deps) | 1 |
| `scripts/fixtures/trait_a.rs` | Self-test fixture: simple trait | 1 |
| `scripts/fixtures/trait_b.rs` | Self-test fixture: generic trait | 1 |
| `artifacts/trait-inventory.md` | 56 trait × 8 signal table | 2 |
| `artifacts/trait-inventory-classified.md` | Above + classification column | 3 |
| `artifacts/deep-review-candidates.md` | 10-15 trait list | 3 |
| `artifacts/deep-reviews/01-{name}.md` ... | One per candidate | 4 |
| `artifacts/recommendations.md` | 3-bucket summary + P0/P1/P2 index | 5 |
| `artifacts/disagreements.md` | 4-party dispute log | 6 |
| `verify.md` | Filled-in evidence | 7 |

No `src/` files are modified.

---

## Task 1: Create extractor script skeleton

**Files:**
- Create: `openspec/changes/2026-06-15-trait-abstraction-review/scripts/extract_trait_signals.sh`

- [ ] **Step 1: Create scripts directory**

```bash
mkdir -p openspec/changes/2026-06-15-trait-abstraction-review/scripts
```

- [ ] **Step 2: Write script header**

Write to `scripts/extract_trait_signals.sh`:

```bash
#!/usr/bin/env bash
# extract_trait_signals.sh
# Extract 8 quantitative signals per `pub trait` in the Synthia workspace.
# Zero external dependencies: requires only bash, rg, awk.
#
# Usage: ./extract_trait_signals.sh [WORKSPACE_ROOT] [OUTPUT_FILE]
#   WORKSPACE_ROOT defaults to current dir's parent (i.e., repo root)
#   OUTPUT_FILE    defaults to artifacts/trait-inventory.md
#
# Output: a markdown table with header + one row per `pub trait`.

set -euo pipefail

WORKSPACE_ROOT="${1:-$(cd "$(dirname "$0")/../../../.." && pwd)}"
OUTPUT_FILE="${2:-$(dirname "$0")/../artifacts/trait-inventory.md}"

if ! command -v rg >/dev/null 2>&1; then
    echo "ERROR: ripgrep (rg) not found in PATH" >&2
    exit 1
fi

if [[ ! -d "$WORKSPACE_ROOT/crates" ]]; then
    echo "ERROR: $WORKSPACE_ROOT does not look like the Synthia workspace" >&2
    exit 1
fi

mkdir -p "$(dirname "$OUTPUT_FILE")"
```

- [ ] **Step 3: Make script executable**

```bash
chmod +x openspec/changes/2026-06-15-trait-abstraction-review/scripts/extract_trait_signals.sh
```

- [ ] **Step 4: Smoke-test script runs without error**

```bash
bash openspec/changes/2026-06-15-trait-abstraction-review/scripts/extract_trait_signals.sh
```

Expected: exits 0, prints no error. The script is incomplete but the header checks pass.

## Task 2: Implement trait declaration discovery

**Files:**
- Modify: `openspec/changes/2026-06-15-trait-abstraction-review/scripts/extract_trait_signals.sh`

- [ ] **Step 1: Append trait-discovery function to the script**

Append after the existing content:

```bash
# Emit a TSV: file<TAB>line<TAB>trait_name<TAB>raw_generic_params
# for every `pub trait` declaration in crates/*/src/**/*.rs.
discover_traits() {
    rg --no-config --line-number --no-heading \
       --glob '!target/**' \
       --glob '!**/target/**' \
       -e '^\s*pub\s+trait\s+\w+' \
       "$WORKSPACE_ROOT/crates" 2>/dev/null \
    | awk -F: '
        {
            file = $1
            line = $2
            raw  = $0
            sub(/^[^:]+:[^:]+:/, "", raw)
            # Extract trait name
            if (match(raw, /pub[[:space:]]+trait[[:space:]]+([A-Za-z0-9_]+)/, m)) {
                name = m[1]
            } else {
                next
            }
            # Extract generic params substring between trait Name and {
            gsub(/^[[:space:]]+/, "", raw)
            gsub(/[[:space:]]*\{.*$/, "", raw)
            printf "%s\t%d\t%s\t%s\n", file, line, name, raw
        }'
}
```

- [ ] **Step 2: Test the discovery on the workspace**

```bash
cd /home/crochee/workspace/synthia
bash openspec/changes/2026-06-15-trait-abstraction-review/scripts/extract_trait_signals.sh 2>&1 | head -5
# Add a debug line temporarily:
discover_traits() { ... ; }
# Then call:
discover_traits | head -5
```

Expected: TSV lines with file, line, name, generic-params-substring. Confirm by eye.

To test, run this one-liner:

```bash
cd /home/crochee/workspace/synthia
rg --no-config --line-number --no-heading -e '^\s*pub\s+trait\s+\w+' crates | head -3
```

Expected: 3+ lines of `path/to/file.rs:LINE:    pub trait TraitName ...`

- [ ] **Step 3: Verify row count == 56**

```bash
cd /home/crochee/workspace/synthia
rg --no-config -e '^\s*pub\s+trait\s+\w+' crates --count-matches
```

Expected: sum across all `*.rs` files equals 56 (matches the project memory baseline).

If count differs, check that the regex still matches all the trait declarations. Investigate any that are missed.

## Task 3: Implement the 8-signal extractors

**Files:**
- Modify: `openspec/changes/2026-06-15-trait-abstraction-review/scripts/extract_trait_signals.sh`

The 8 signals per trait:

| Signal | Function name | Method |
|--------|---------------|--------|
| `impl_count` | `sig_impl_count` | `rg -c "impl[^;]*for ${name}\\b"` across all crates |
| `method_count` | `sig_method_count` | `awk` count of `fn` inside the trait body (between `pub trait NAME` and matching `}`) |
| `generic_params` | `sig_generic_params` | count of `,`+1 in `<...>` substring of the trait header |
| `lifetime_params` | `sig_lifetime_params` | count of `'` occurrences in `<...>` substring |
| `associated_types` | `sig_assoc_types` | `rg -c "^[[:space:]]*type [A-Z]" ${file}` within the trait body |
| `call_sites` | `sig_call_sites` | `rg -c "as ${name}\b\|dyn ${name}\b" crates` |
| `dyn_usage` | `sig_dyn_usage` | `rg -c "dyn ${name}\b" crates` |
| `file_size_lines` | `sig_file_size_lines` | number of lines from trait `{` to matching `}` |

- [ ] **Step 1: Append the 8 signal functions**

```bash
# Count how many `impl` blocks implement the given trait.
sig_impl_count() {
    local name="$1"
    rg --no-config --glob '!target/**' "impl[[:space:]]+[^;{]*[[:space:]]+for[[:space:]]+${name}\b" "$WORKSPACE_ROOT/crates" 2>/dev/null | wc -l | tr -d ' '
}

# Count `fn` lines between the trait `{` and its matching `}`.
# Uses awk to track brace depth, then counts `fn` lines.
sig_method_count() {
    local file="$1" start_line="$2"
    awk -v start="$start_line" '
        NR < start { next }
        /\{/ { for (i=1; i<=length($0); i++) if (substr($0,i,1)=="{") depth++ }
        /\}/ {
            for (i=1; i<=length($0); i++) {
                if (substr($0,i,1)=="{") depth++
                else if (substr($0,i,1)=="}") {
                    depth--
                    if (depth < 0) { exit }
                }
            }
        }
        /\bfn[[:space:]]+[A-Za-z0-9_]+/ { count++ }
        END { print count+0 }
    ' "$file"
}

# Count generic type parameters (T, U, ...) in the header substring.
sig_generic_params() {
    local raw="$1"
    # Strip lifetimes first to avoid double-counting.
    local no_lt
    no_lt=$(printf '%s' "$raw" | sed -E "s/'[A-Za-z_][A-Za-z0-9_]*//g")
    # Extract content within <...>
    local inside
    inside=$(printf '%s' "$no_lt" | sed -nE 's/.*<([^>]+)>.*/\1/p')
    if [[ -z "$inside" ]]; then echo 0; return; fi
    # Count top-level commas (not inside nested <...>).
    local depth=0 commas=0 i ch
    for (( i=0; i<${#inside}; i++ )); do
        ch="${inside:$i:1}"
        case "$ch" in
            "<") (( depth++ )) ;;
            ">") (( depth-- )) ;;
            ",") (( depth == 0 )) && (( commas++ )) ;;
        esac
    done
    echo $(( commas + 1 ))
}

sig_lifetime_params() {
    local raw="$1"
    local inside
    inside=$(printf '%s' "$raw" | sed -nE 's/.*<([^>]+)>.*/\1/p')
    if [[ -z "$inside" ]]; then echo 0; return; fi
    # Count '\'' followed by identifier-start char.
    printf '%s' "$inside" | grep -oE "'[A-Za-z_][A-Za-z0-9_]*" | wc -l | tr -d ' '
}

# Count `type Foo = ...;` / `type Foo;` lines in the trait body.
sig_assoc_types() {
    local file="$1" start_line="$2"
    awk -v start="$start_line" '
        NR < start { next }
        /\{/ { for (i=1; i<=length($0); i++) if (substr($0,i,1)=="{") depth++ }
        /\}/ {
            for (i=1; i<=length($0); i++) {
                if (substr($0,i,1)=="{") depth++
                else if (substr($0,i,1)=="}") {
                    depth--
                    if (depth < 0) { exit }
                }
            }
        }
        /^[[:space:]]*type[[:space:]]+[A-Z]/ { count++ }
        END { print count+0 }
    ' "$file"
}

sig_call_sites() {
    local name="$1"
    {
        rg --no-config --glob '!target/**' "as[[:space:]]+${name}\b" "$WORKSPACE_ROOT/crates" 2>/dev/null
        rg --no-config --glob '!target/**' "dyn[[:space:]]+${name}\b" "$WORKSPACE_ROOT/crates" 2>/dev/null
    } | wc -l | tr -d ' '
}

sig_dyn_usage() {
    local name="$1"
    rg --no-config --glob '!target/**' "dyn[[:space:]]+${name}\b" "$WORKSPACE_ROOT/crates" 2>/dev/null \
        | wc -l | tr -d ' '
}

# Body size in lines: from trait { line to matching } line.
sig_file_size_lines() {
    local file="$1" start_line="$2"
    awk -v start="$start_line" '
        NR < start { next }
        {
            for (i=1; i<=length($0); i++) {
                ch = substr($0, i, 1)
                if (ch == "{") depth++
                else if (ch == "}") {
                    depth--
                    if (depth < 0) { print NR - start + 1; exit }
                }
            }
        }
        END { if (depth < 0) {} else { print "open" } }
    ' "$file"
}
```

- [ ] **Step 2: Manually verify one signal on a known trait**

Pick a known trait, e.g. `Provider` in synthia-provider. Then run:

```bash
cd /home/crochee/workspace/synthia
# Find Provider
rg --no-config -e '^\s*pub\s+trait\s+Provider\b' crates
# Should show file:line. Then verify impl count:
rg --no-config "impl[[:space:]]+[^;{]*[[:space:]]+for[[:space:]]+Provider\b" crates | wc -l
```

Expected: matches the count in your inventory. Compare against the project's known count of Provider impls (e.g., 3+).

## Task 4: Implement markdown table output

**Files:**
- Modify: `openspec/changes/2026-06-15-trait-abstraction-review/scripts/extract_trait_signals.sh`

- [ ] **Step 1: Append the main pipeline + table output**

```bash
# Main: write a markdown table with header + 56 data rows.
main() {
    local header="# Trait Inventory (auto-generated, 8 signals per pub trait)
> Generated by extract_trait_signals.sh on $(date -u +%Y-%m-%dT%H:%M:%SZ)
> Workspace: \`${WORKSPACE_ROOT}\`

| trait | file:line | impl | methods | generics | lifetimes | assoc_types | call_sites | dyn | body_lines |
|-------|-----------|------|---------|----------|-----------|-------------|------------|-----|------------|"

    local rows=()
    while IFS=$'\t' read -r file line name raw; do
        local impl_count method_count gen_params lt_params assoc_types call_sites dyn body_lines
        impl_count=$(sig_impl_count "$name")
        method_count=$(sig_method_count "$file" "$line")
        gen_params=$(sig_generic_params "$raw")
        lt_params=$(sig_lifetime_params "$raw")
        assoc_types=$(sig_assoc_types "$file" "$line")
        call_sites=$(sig_call_sites "$name")
        dyn=$(sig_dyn_usage "$name")
        body_lines=$(sig_file_size_lines "$file" "$line")

        # Convert file path to repo-relative.
        local rel="${file#${WORKSPACE_ROOT}/}"
        rows+=("| \`${name}\` | \`${rel}:${line}\` | ${impl_count} | ${method_count} | ${gen_params} | ${lt_params} | ${assoc_types} | ${call_sites} | ${dyn} | ${body_lines} |")
    done < <(discover_traits)

    {
        echo "$header"
        printf '%s\n' "${rows[@]}"
    } > "$OUTPUT_FILE"

    local n=${#rows[@]}
    echo "Wrote $n trait rows to $OUTPUT_FILE"
}

main "$@"
```

- [ ] **Step 2: Run the script**

```bash
cd /home/crochee/workspace/synthia
bash openspec/changes/2026-06-15-trait-abstraction-review/scripts/extract_trait_signals.sh
```

Expected: `Wrote 56 trait rows to .../artifacts/trait-inventory.md`

- [ ] **Step 3: Spot-check the output**

```bash
cd /home/crochee/workspace/synthia
head -3 openspec/changes/2026-06-15-trait-abstraction-review/artifacts/trait-inventory.md
wc -l openspec/changes/2026-06-15-trait-abstraction-review/artifacts/trait-inventory.md
```

Expected: header line + separator + first trait row, total line count = 1 (header) + 1 (separator) + 56 = 58 lines (or 60 with trailing newline).

- [ ] **Step 4: Verify 8 columns per row**

```bash
cd /home/crochee/workspace/synthia
awk -F'|' 'NR > 2 { print NF-1 }' openspec/changes/2026-06-15-trait-abstraction-review/artifacts/trait-inventory.md | sort -u
```

Expected: only `10` (because `|` delimiters produce NF=10, NF-1=9 data columns; the first/last are empty because of leading/trailing `|`). If you see any other number, the script has a bug.

## Task 5: Self-test fixtures

**Files:**
- Create: `openspec/changes/2026-06-15-trait-abstraction-review/scripts/fixtures/synthetic_crate/src/lib.rs`

- [ ] **Step 1: Create fixture directory structure**

```bash
mkdir -p openspec/changes/2026-06-15-trait-abstraction-review/scripts/fixtures/synthetic_crate/src
```

- [ ] **Step 2: Write fixture trait A (simple, 1 impl)**

Write to `scripts/fixtures/synthetic_crate/src/lib.rs`:

```rust
// Fixture for extract_trait_signals.sh self-test.
// Defines traits with KNOWN signal counts to verify the extractor.

// Trait A: 1 impl, 2 methods, 0 generics, 0 lifetimes, 0 assoc_types, 1 call site, 0 dyn
pub trait FixtureTraitA {
    fn alpha(&self) -> i32;
    fn beta(&mut self, x: i32);
}

pub struct FixtureStructA;
impl FixtureTraitA for FixtureStructA {
    fn alpha(&self) -> i32 { 1 }
    fn beta(&mut self, _x: i32) {}
}

#[allow(dead_code)]
fn _use_a() {
    let mut s = FixtureStructA;
    let _ = s.alpha();
    s.beta(0);
}
```

- [ ] **Step 3: Write fixture trait B (generic, 1 impl, 1 call site, 0 dyn)**

Append to the same file:

```rust

// Trait B: 1 impl, 1 method, 1 generic, 0 lifetimes, 0 assoc_types, 0 call sites, 0 dyn
pub trait FixtureTraitB<T: Clone> {
    fn transform(&self, input: T) -> T;
}

pub struct FixtureStructB;
impl<T: Clone + Default> FixtureTraitB<T> for FixtureStructB {
    fn transform(&self, input: T) -> T { input }
}
```

- [ ] **Step 4: Self-test the extractor on fixtures**

```bash
cd /home/crochee/workspace/synthia
# Run the extractor on the synthetic crate (smaller scope for self-test).
mkdir -p /tmp/fixture-inv
# Strip workspace check temporarily by symlinking crates → the synthetic_crate:
bash -c '
WORKSPACE=/tmp/fixture-inv-ws
rm -rf "$WORKSPACE"
mkdir -p "$WORKSPACE/crates"
cp -r openspec/changes/2026-06-15-trait-abstraction-review/scripts/fixtures/synthetic_crate "$WORKSPACE/crates/synthia_fixture"
OUT=/tmp/fixture-inv/inventory.md
bash openspec/changes/2026-06-15-trait-abstraction-review/scripts/extract_trait_signals.sh "$WORKSPACE" "$OUT"
echo "---"
cat "$OUT"
'
```

Expected: 2 rows (FixtureTraitA and FixtureTraitB) with the expected signal counts:
- FixtureTraitA: impl=1, methods=2, generics=0, lifetimes=0, assoc=0, calls=1, dyn=0
- FixtureTraitB: impl=1, methods=1, generics=1, lifetimes=0, assoc=0, calls=0, dyn=0

If the actual values differ, debug individual signal functions.

- [ ] **Step 5: Document the self-test in the script**

Append a comment to `scripts/extract_trait_signals.sh`:

```bash
# Self-test:
#   $ bash scripts/extract_trait_signals.sh <workspace> <output>
# where <workspace>/crates/synthia_fixture contains scripts/fixtures/synthetic_crate
# Expected: 2 trait rows matching the comment block in fixtures/synthetic_crate/src/lib.rs.
```

## Task 6: Synthetic-drift regression test

**Files:**
- Modify: `openspec/changes/2026-06-15-trait-abstraction-review/scripts/extract_trait_signals.sh`

The CI script `scripts/check_synced_spec_format.sh` uses a "synthetic drift" pattern: it injects a known-wrong file and verifies the script catches it. Replicate that pattern here.

- [ ] **Step 1: Add a synthetic-drift test fixture**

Create `scripts/fixtures/drift_crate/src/lib.rs`:

```bash
mkdir -p openspec/changes/2026-06-15-trait-abstraction-review/scripts/fixtures/drift_crate/src
```

```rust
// Drift fixture: missing a `pub` keyword on one trait. The extractor
// regex (^\s*pub\s+trait\s+) should NOT match it. Verify the row count
// is 1 (not 2).
trait NonPubTrait {
    fn should_not_appear(&self);
}

pub struct DriftImpl;
impl NonPubTrait for DriftImpl {
    fn should_not_appear(&self) {}
}

pub trait DriftPubTrait {
    fn should_appear(&self) -> i32;
}
pub struct DriftImpl2;
impl DriftPubTrait for DriftImpl2 {
    fn should_appear(&self) -> i32 { 0 }
}
```

- [ ] **Step 2: Verify drift detection**

```bash
cd /home/crochee/workspace/synthia
bash -c '
WORKSPACE=/tmp/drift-ws
rm -rf "$WORKSPACE"
mkdir -p "$WORKSPACE/crates"
cp -r openspec/changes/2026-06-15-trait-abstraction-review/scripts/fixtures/drift_crate "$WORKSPACE/crates/synthia_drift"
OUT=/tmp/drift-inv/inventory.md
bash openspec/changes/2026-06-15-trait-abstraction-review/scripts/extract_trait_signals.sh "$WORKSPACE" "$OUT"
echo "---"
cat "$OUT"
'
```

Expected: only 1 row (DriftPubTrait). The NonPubTrait should NOT appear (because the regex requires `pub trait`).

- [ ] **Step 3: Add drift test documentation**

Append to the self-test comment in the script:

```bash
# Synthetic-drift test:
#   Same setup as self-test, but use fixtures/drift_crate which has a non-pub trait.
#   Expected: 1 row (DriftPubTrait only). NonPubTrait MUST be excluded.
```

## Task 7: Generate classified inventory (Phase 3)

**Files:**
- Create: `openspec/changes/2026-06-15-trait-abstraction-review/artifacts/trait-inventory-classified.md`
- Create: `openspec/changes/2026-06-15-trait-abstraction-review/artifacts/deep-review-candidates.md`

- [ ] **Step 1: Read the inventory and apply the decision matrix**

```bash
cd /home/crochee/workspace/synthia
INV=openspec/changes/2026-06-15-trait-abstraction-review/artifacts/trait-inventory.md
# Inspect
cat "$INV" | head -10
```

- [ ] **Step 2: Manually apply the decision matrix to each row**

The matrix (from design.md §3):

```
| impl | calls | generic | 类别       | deep review? |
| 1    | <3    | 0       | REMOVE_CANDIDATE | yes |
| 1    | >=3   | any     | REVIEW     | yes |
| 1    | any   | >=2     | REVIEW     | yes |
| 2+   | any   | <2      | KEEP       | skip |
| 2+   | any   | >=2     | REVIEW     | yes |
| 2+   | high  | 0       | KEEP       | skip |
| 0 calls| any  | any     | KEEP-dead? | check dyn |
```

For each of the 56 rows, decide one of: KEEP, REVIEW, REMOVE_CANDIDATE.

- [ ] **Step 3: Append the classification column to a new file**

Write to `artifacts/trait-inventory-classified.md`:

```bash
INV=openspec/changes/2026-06-15-trait-abstraction-review/artifacts/trait-inventory.md
OUT=openspec/changes/2026-06-15-trait-abstraction-review/artifacts/trait-inventory-classified.md
# Header: original + new "category" column
{
    head -2 "$INV"
    echo "| trait | file:line | impl | methods | generics | lifetimes | assoc_types | call_sites | dyn | body_lines | category |"
    echo "|-------|-----------|------|---------|----------|-----------|-------------|------------|-----|------------|----------|"
    # Skip the original header rows and the separator (lines 1-2), keep data rows.
    tail -n +3 "$INV" | awk -F'|' '{
        # Extract fields: $2=trait, $3=file:line, $4=impl, $5=methods,
        # $6=generics, $7=lifetimes, $8=assoc, $9=calls, $10=dyn, $11=body
        impl=$4; calls=$9; gen=$6;
        category="";
        if (impl == 0) {
            # No impls: keep-dead candidate. Check dyn.
            if ($10+0 == 0) category="KEEP-dead?";
            else category="KEEP";
        } else if (impl == 1) {
            if (calls+0 < 3 && gen+0 == 0) category="REMOVE_CANDIDATE";
            else if (calls+0 >= 3) category="REVIEW";
            else if (gen+0 >= 2) category="REVIEW";
            else category="REVIEW";
        } else { # impl >= 2
            if (gen+0 >= 2) category="REVIEW";
            else category="KEEP";
        }
        print $0 " " category "|"
    }'
} > "$OUT"
```

- [ ] **Step 4: Verify classification counts sum to 56**

```bash
cd /home/crochee/workspace/synthia
INV=openspec/changes/2026-06-15-trait-abstraction-review/artifacts/trait-inventory-classified.md
echo "KEEP: $(grep -c '| KEEP ' "$INV")"
echo "KEEP-dead?: $(grep -c '| KEEP-dead' "$INV")"
echo "REVIEW: $(grep -c '| REVIEW ' "$INV")"
echo "REMOVE_CANDIDATE: $(grep -c '| REMOVE_CANDIDATE ' "$INV")"
echo "TOTAL: $(tail -n +3 "$INV" | wc -l)"
```

Expected: sum of all categories == 56.

- [ ] **Step 5: Generate the deep-review candidates list**

Write to `artifacts/deep-review-candidates.md`:

```bash
cd /home/crochee/workspace/synthia
INV=openspec/changes/2026-06-15-trait-abstraction-review/artifacts/trait-inventory-classified.md
OUT=openspec/changes/2026-06-15-trait-abstraction-review/artifacts/deep-review-candidates.md
{
    echo "# Deep-review candidates"
    echo
    echo "Auto-selected from decision matrix. Cap = 15."
    echo
    echo "| # | trait | file:line | impl | calls | generics | category |"
    echo "|---|-------|-----------|------|-------|----------|----------|"
    grep -E '\| (REVIEW|REMOVE_CANDIDATE) \|' "$INV" \
        | head -15 \
        | awk -F'|' '{
            n=NR
            trait=$2; file=$3; impl=$4; calls=$9; gen=$6; cat=$13
            printf "| %02d | %s | %s | %s | %s | %s | %s |\n", n, trait, file, impl, calls, gen, cat
        }'
} > "$OUT"

cat "$OUT"
```

Expected: between 1 and 15 rows. If zero, decision matrix is wrong (too generous). If >15, cap at 15 (already done by `head -15`).

## Task 8: Deep review template (Task 4.1)

**Files:**
- Create: `openspec/changes/2026-06-15-trait-abstraction-review/artifacts/deep-reviews/00-TEMPLATE.md`

- [ ] **Step 1: Create deep-reviews directory**

```bash
mkdir -p openspec/changes/2026-06-15-trait-abstraction-review/artifacts/deep-reviews
```

- [ ] **Step 2: Write the template file**

Write to `artifacts/deep-reviews/00-TEMPLATE.md`:

```markdown
# Deep Review: {TraitName}

**Location**: `crates/.../foo.rs:N`
**Signals**: {impl} impl / {methods} methods / {gen} generics / {calls} call sites / {dyn} dyn

## 目的
{1-2 句:从 doc comment + 实际 usage 推断这 trait 解决什么问题}

## 存在价值
{解释 why this trait vs 直接用具体类型。列出至少 1 个具体使用场景 (文件:行号)}

## 替代方案
- **A) 直接用具体类型** (无 trait)。代码量减少, 但失去多态能力
- **B) 保留 trait + 简化方法集**。如果 method_count > 5, 看是否职责过宽
- **C) 拆为多个小 trait** (接口隔离)。如果 generic_params >= 2, 看是否能拆

## 推荐
**{KEEP | REVIEW | REMOVE_CANDIDATE}**

## 理由
{2-3 句,基于具体证据 (impl 数 / 调用点 / 历史 commit / 未来 plan)。
例如: "impl=1 + call_sites=0, 但有 dyn 引用 → KEEP 但需记录"}
```

## Task 9: Write first deep review (Task 4.2)

**Files:**
- Create: `openspec/changes/2026-06-15-trait-abstraction-review/artifacts/deep-reviews/01-{name}.md`

The first candidate should be the one most clearly classified (e.g., a 1-impl / 0-call-sites trait that's an obvious REMOVE_CANDIDATE). This task demonstrates the full workflow.

- [ ] **Step 1: Pick the first candidate**

```bash
cd /home/crochee/workspace/synthia
cat openspec/changes/2026-06-15-trait-abstraction-review/artifacts/deep-review-candidates.md
```

Pick the first row, e.g. `01-FirstTraitName.md`.

- [ ] **Step 2: Read the trait's source and 3 call sites (or "no call sites")**

```bash
cd /home/crochee/workspace/synthia
# Open the trait file at the line shown in the inventory.
${EDITOR:-nano} crates/path/to/trait.rs
# Note: line number, doc comment, all method signatures.
```

- [ ] **Step 3: Apply the 4-party adversarial review**

For each of the 4 parties (怀疑派 / 架构派 / 生产派 / 简化派), write 1-2 sentences with their stance and reasoning. Then declare consensus (≥ 3 parties agree).

- [ ] **Step 4: Write the review file**

Write to `artifacts/deep-reviews/01-{TraitName}.md`, populating the template. Add a 4-party section:

```markdown
## 4-party 检查

- **怀疑派** (默认移除): {立场 + 论证}
- **架构派** (依赖倒置): {立场 + 论证}
- **生产派** (影响面): {立场 + 论证}
- **简化派** (更简单的抽象): {立场 + 论证}

**共识**: {N 派同意 / 分歧记录}
```

## Task 10: Complete remaining 9-14 deep reviews (Tasks 4.3-4.16)

**Files:**
- Create: `openspec/changes/2026-06-15-trait-abstraction-review/artifacts/deep-reviews/{02..15}-{name}.md`

- [ ] **Step 1-5: For each remaining candidate, repeat Task 9 steps 1-5**

For each candidate in `deep-review-candidates.md` (excluding the one done in Task 9):
1. Read trait source
2. Apply 4-party review
3. Write file `NN-{TraitName}.md`
4. Commit
5. Move to next

Expected total: 10-15 deep review files. Cap at 15 (if more than 15 candidates, prioritize REMOVE_CANDIDATE > REVIEW).

---

## Task 11: Synthesize recommendations (Phase 5)

**Files:**
- Create: `openspec/changes/2026-06-15-trait-abstraction-review/artifacts/recommendations.md`

- [ ] **Step 1: Count each category**

```bash
cd /home/crochee/workspace/synthia
INV=openspec/changes/2026-06-15-trait-abstraction-review/artifacts/trait-inventory-classified.md
echo "KEEP: $(grep -c '| KEEP ' "$INV")"
echo "REVIEW: $(grep -c '| REVIEW ' "$INV")"
echo "REMOVE_CANDIDATE: $(grep -c '| REMOVE_CANDIDATE ' "$INV")"
```

- [ ] **Step 2: Identify typical examples per category**

```bash
cd /home/crochee/workspace/synthia
INV=openspec/changes/2026-06-15-trait-abstraction-review/artifacts/trait-inventory-classified.md
echo "Top 5 KEEP traits (highest impl count):"
grep '| KEEP ' "$INV" | sort -t'|' -k4 -n -r | head -5
echo "All REMOVE_CANDIDATE:"
grep '| REMOVE_CANDIDATE ' "$INV"
```

- [ ] **Step 3: Write recommendations.md**

Write to `artifacts/recommendations.md`:

```markdown
# Recommendations: trait-abstraction-review

> Generated: $(date -u +%Y-%m-%d)
> Source: trait-inventory-classified.md (56 traits) + 10-15 deep reviews

## Summary

| Category | Count | % of total |
|----------|-------|------------|
| KEEP | {N} | {%} |
| REVIEW | {N} | {%} |
| REMOVE_CANDIDATE | {N} | {%} |
| KEEP-dead? | {N} | {%} |
| **Total** | **56** | **100%** |

## Typical representatives

### KEEP (3-5 examples)
- `{TraitName}` ({file:line}): {1 句 — 为什么保留}
- ...

### REVIEW (full list)
- `{TraitName}` ({file:line}): see [deep-reviews/NN-{name}.md](../deep-reviews/NN-{name}.md)
- ...

### REMOVE_CANDIDATE (full list)
- `{TraitName}` ({file:line}): see [deep-reviews/NN-{name}.md](../deep-reviews/NN-{name}.md)
- ...

## Future refactor candidates

| Priority | Trait | Reason |
|----------|-------|--------|
| P0 | `{name}` (impl=1, calls=0) | dead abstraction |
| P1 | `{name}` (methods>8) | 职责过宽 |
| P1 | `{name}` (generics>=2) | 抽象过载 |
| P2 | `{name}` (other REVIEW) | 留待 future change 评估 |
```

Populate the table with the actual candidates from your classification.

- [ ] **Step 4: Verify count math**

```bash
cd /home/crochee/workspace/synthia
grep -E "^\| KEEP \|" openspec/changes/2026-06-15-trait-abstraction-review/artifacts/recommendations.md
grep -E "^\| REVIEW \|" openspec/changes/2026-06-15-trait-abstraction-review/artifacts/recommendations.md
grep -E "^\| REMOVE_CANDIDATE \|" openspec/changes/2026-06-15-trait-abstraction-review/artifacts/recommendations.md
```

Verify: KEEP + REVIEW + REMOVE_CANDIDATE = 56

## Task 12: 4-party adversarial review of full report (Phase 6)

**Files:**
- Create: `openspec/changes/2026-06-15-trait-abstraction-review/artifacts/disagreements.md`

- [ ] **Step 1: Re-read all artifacts**

```bash
cd /home/crochee/workspace/synthia
ls openspec/changes/2026-06-15-trait-abstraction-review/artifacts/
cat openspec/changes/2026-06-15-trait-abstraction-review/artifacts/recommendations.md
```

- [ ] **Step 2: Apply 4-party review**

For each deep review, check:
- 怀疑派: 能否用直接类型替代?
- 架构派: 是否过度抽象?
- 生产派: 移除/重构的影响面?
- 简化派: 能否用闭包/newtype 替代?

If all 4 parties agree: KEEP / REVIEW / REMOVE_CANDIDATE is consensus.
If disagreement: log in disagreements.md.

- [ ] **Step 3: Write disagreements.md (only if any)**

Write to `artifacts/disagreements.md`:

```markdown
# 4-party Disagreements

> Empty if all parties agree on all classifications.

## {TraitName}

**Classification**: {proposed}

**Disagreement**:
- 怀疑派: {stance}
- 简化派: {stance}
- 架构派: {stance}
- 生产派: {stance}

**Resolution**: {deferred to future change / adopted majority view}
```

(If all parties agree, write `# No disagreements` and commit anyway.)

## Task 13: Fill verify.md and validate (Phase 7)

**Files:**
- Modify: `openspec/changes/2026-06-15-trait-abstraction-review/verify.md`

- [ ] **Step 1: Update verify.md with actual evidence**

Replace the placeholder content of `verify.md` with:

```markdown
# Verify: trait-abstraction-review

> Written: $(date -u +%Y-%m-%d)
> Status: Complete

## 0. Evidence

- 7 阶段全部完成
- {N} 个 deep-review 文件
- {N} 个 disagreements (或 'no disagreements')

## 1. 7 阶段执行记录

### Phase 1 - 采集脚本
- 脚本: `scripts/extract_trait_signals.sh`
- Self-test: clean + synthetic drift 双路径通过

### Phase 2 - 全量扫描
- 输出: `artifacts/trait-inventory.md` ({N} 行 + header + separator)

### Phase 3 - 决策矩阵
- KEEP: {N}
- REVIEW: {N}
- REMOVE_CANDIDATE: {N}
- 总和: 56 ✓

### Phase 4 - 深度 review
- 文件数: {N} (10-15 之间)
- 4-party 共识: 100%

### Phase 5 - 汇总
- `artifacts/recommendations.md` 含 P0/P1/P2 索引

### Phase 6 - 对抗
- 4-party 全文审查: {N} 个 disagreements (或 0)

### Phase 7 - 验收
- `openspec validate 2026-06-15-trait-abstraction-review`: 通过

## 2. 自检清单

- [x] 零新依赖
- [x] `src/` 0 改动 (git diff crates/*/src/ 为空)
- [x] KEEP + REVIEW + REMOVE_CANDIDATE = 56
- [x] deep-reviews 文件数在 10-15 之间
- [x] 每篇 deep review 4-party ≥ 3 派同意
- [x] recommendations.md 含 P0/P1/P2 索引
- [x] `openspec validate` 通过
```

- [ ] **Step 2: Run openspec validate**

```bash
cd /home/crochee/workspace/synthia
openspec validate 2026-06-15-trait-abstraction-review
```

Expected: `Change '2026-06-15-trait-abstraction-review' is valid`

- [ ] **Step 3: Run the synced-spec-format CI gate**

```bash
cd /home/crochee/workspace/synthia
bash scripts/check_synced_spec_format.sh
```

Expected: `OK: 61 synced specs are in cumulative format (no delta headers).` (or 62 if this change's synced spec has been written, but since this is a research change with no synced spec, it should stay 61).

- [ ] **Step 4: Verify zero source-code changes**

```bash
cd /home/crochee/workspace/synthia
git diff --stat crates/*/src/
```

Expected: empty output.

---

## Self-Review Checklist

After completing all tasks, verify:

- [ ] Spec coverage: All 5 requirements in `spec.md` are addressed (inventory, matrix, deep-reviews, index, zero-source)
- [ ] No placeholders: Search for TBD/TODO/fill in this plan
- [ ] Type consistency: `extract_trait_signals.sh` signatures match across tasks
- [ ] File paths: All paths use `openspec/changes/2026-06-15-trait-abstraction-review/` prefix
- [ ] Counts: KEEP + REVIEW + REMOVE_CANDIDATE = 56
- [ ] `openspec validate` passes
- [ ] `src/` is untouched
- [ ] Self-test (clean) passes
- [ ] Self-test (synthetic drift) passes

---

## Execution Time Budget

- Phase 1 (Tasks 1-6): ~50m
- Phase 2 (Task 7): ~10m
- Phase 3-4 (Tasks 8-10): ~80m (10-15 reviews × 5m)
- Phase 5 (Task 11): ~15m
- Phase 6 (Task 12): ~15m
- Phase 7 (Task 13): ~10m
- **Total**: ~3h

If a task exceeds its expected time, defer to a follow-up change rather than scope-creep.
