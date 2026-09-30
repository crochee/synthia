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

# Emit a TSV: file<TAB>line<TAB>trait_name<TAB>raw_generic_params
# for every `pub trait` declaration in crates/*/src/**/*.rs.
discover_traits() {
    { rg --no-config --line-number --no-heading \
       --glob '!target/**' \
       --glob '!**/target/**' \
       -e '^\s*pub\s+trait\s+\w+' \
       "$WORKSPACE_ROOT/crates" 2>/dev/null || true; } \
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

# Count how many `impl` blocks implement the given trait.
# Handles: `impl Foo for Bar`, `impl crate::traits::Foo for Bar`,
# `impl<T> Foo<T> for Bar`, `impl Foo<T> for Bar`.
sig_impl_count() {
    local name="$1"
    { rg --no-config --glob '!target/**' "impl[^;{]*\\b${name}\\b[^;{]*for[[:space:]]+[A-Za-z0-9_]" "$WORKSPACE_ROOT/crates" 2>/dev/null || true; } | wc -l | tr -d ' '
}

# Count `fn` lines between the trait `{` and its matching `}`.
sig_method_count() {
    local file="$1" start_line="$2"
    awk -v start="$start_line" '
        NR < start { next }
        NR == start {
            for (i=1; i<=length($0); i++) {
                if (substr($0,i,1) == "{") depth++
                else if (substr($0,i,1) == "}") depth--
            }
            if (match($0, /(^|[^A-Za-z0-9_])fn[[:space:]]+[A-Za-z0-9_]+/)) count++
            next
        }
        {
            for (i=1; i<=length($0); i++) {
                ch = substr($0, i, 1)
                if (ch == "{") depth++
                else if (ch == "}") {
                    depth--
                    if (depth <= 0) { print count+0; exit }
                }
            }
            if (match($0, /(^|[^A-Za-z0-9_])fn[[:space:]]+[A-Za-z0-9_]+/)) count++
        }
        END { if (depth > 0) print count+0 }
    ' "$file"
}

# Count generic type parameters (T, U, ...) in the header substring (after stripping lifetimes).
# Approach: strip lifetimes, extract content inside <>, strip leading comma+spaces
# (artifacts of stripped lifetimes), then count remaining top-level commas + 1.
sig_generic_params() {
    local raw="$1"
    local no_lt
    no_lt=$(printf '%s' "$raw" | sed -E "s/'[A-Za-z_][A-Za-z0-9_]*//g")
    local inside
    inside=$(printf '%s' "$no_lt" | sed -nE 's/.*<([^>]+)>.*/\1/p')
    if [[ -z "$inside" ]]; then echo 0; return; fi
    # Strip leading comma+space groups (artifacts of stripped lifetimes).
    # Pattern matches one or more groups of (spaces + commas + spaces) anchored at ^.
    inside=$(printf '%s' "$inside" | sed -E 's/^([[:space:]]*,+[[:space:]]*)+//')
    # Count top-level commas (assuming no nested generics in trait headers).
    local commas
    commas=$(printf '%s' "$inside" | tr -cd ',' | wc -c | tr -d ' ')
    echo $(( commas + 1 ))
}

sig_lifetime_params() {
    local raw="$1"
    local inside
    inside=$(printf '%s' "$raw" | sed -nE 's/.*<([^>]+)>.*/\1/p')
    if [[ -z "$inside" ]]; then echo 0; return; fi
    { printf '%s' "$inside" | grep -oE "'[A-Za-z_][A-Za-z0-9_]*" || true; } | wc -l | tr -d ' '
}

# Count `type Foo = ...;` / `type Foo;` lines in the trait body.
sig_assoc_types() {
    local file="$1" start_line="$2"
    awk -v start="$start_line" '
        NR < start { next }
        NR == start {
            for (i=1; i<=length($0); i++) {
                if (substr($0,i,1)=="{") depth++
                else if (substr($0,i,1)=="}") depth--
            }
            if (match($0, /(^|[[:space:]])type[[:space:]]+[A-Z]/)) count++
            next
        }
        {
            for (i=1; i<=length($0); i++) {
                if (substr($0,i,1)=="{") depth++
                else if (substr($0,i,1)=="}") {
                    depth--
                    if (depth <= 0) { print count+0; exit }
                }
            }
            if (match($0, /(^|[[:space:]])type[[:space:]]+[A-Z]/)) count++
        }
        END { if (depth > 0) print count+0 }
    ' "$file"
}

sig_call_sites() {
    local name="$1"
    {
        # Filter out `use Foo as Bar` import aliases (false positives).
        # We use --multiline-dotall trick? No — just exclude lines starting with `use`.
        { rg --no-config --glob '!target/**' "as[[:space:]]+[A-Za-z0-9_:]*${name}\b" "$WORKSPACE_ROOT/crates" 2>/dev/null \
            | rg -v '^\s*use\b' || true; }
        { rg --no-config --glob '!target/**' "dyn[[:space:]]+[A-Za-z0-9_:]*${name}\b" "$WORKSPACE_ROOT/crates" 2>/dev/null || true; }
    } | wc -l | tr -d ' '
}

sig_dyn_usage() {
    local name="$1"
    { rg --no-config --glob '!target/**' "dyn[[:space:]]+[A-Za-z0-9_:]*${name}\b" "$WORKSPACE_ROOT/crates" 2>/dev/null || true; } \
        | wc -l | tr -d ' '
}

# Body size in lines: from trait { line to matching } line.
sig_file_size_lines() {
    local file="$1" start_line="$2"
    awk -v start="$start_line" '
        NR < start { next }
        NR == start {
            for (i=1; i<=length($0); i++) {
                if (substr($0,i,1) == "{") depth++
                else if (substr($0,i,1) == "}") depth--
            }
            next
        }
        {
            for (i=1; i<=length($0); i++) {
                ch = substr($0, i, 1)
                if (ch == "{") depth++
                else if (ch == "}") {
                    depth--
                    if (depth <= 0) { print NR - start + 1; exit }
                }
            }
        }
        END { if (depth > 0) print "open" }
    ' "$file"
}

# Main: write a markdown table with header + N data rows.
main() {
    local ts
    ts=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    local header="# Trait Inventory (auto-generated, 8 signals per pub trait)
> Generated by extract_trait_signals.sh on ${ts}
> Workspace: \`${WORKSPACE_ROOT}\`

| trait | file:line | impl | methods | generics | lifetimes | assoc_types | call_sites | dyn | body_lines |
|-------|-----------|------|---------|----------|-----------|-------------|------------|-----|------------|"

    local tmp_rows
    tmp_rows=$(mktemp)
    local n=0
    while IFS=$'\t' read -r file line name raw; do
        [[ -z "$name" ]] && continue
        local impl_count method_count gen_params lt_params assoc_types call_sites dyn body_lines
        impl_count=$(sig_impl_count "$name")
        method_count=$(sig_method_count "$file" "$line")
        gen_params=$(sig_generic_params "$raw")
        lt_params=$(sig_lifetime_params "$raw")
        assoc_types=$(sig_assoc_types "$file" "$line")
        call_sites=$(sig_call_sites "$name")
        dyn=$(sig_dyn_usage "$name")
        body_lines=$(sig_file_size_lines "$file" "$line")

        local rel="${file#${WORKSPACE_ROOT}/}"
        printf '| `%s` | `%s:%s` | %s | %s | %s | %s | %s | %s | %s | %s |\n' \
            "$name" "$rel" "$line" \
            "$impl_count" "$method_count" "$gen_params" "$lt_params" \
            "$assoc_types" "$call_sites" "$dyn" "$body_lines" \
            >> "$tmp_rows"
        n=$(( n + 1 ))
    done < <(discover_traits)

    {
        echo "$header"
        cat "$tmp_rows"
    } > "$OUTPUT_FILE"
    rm -f "$tmp_rows"

    echo "Wrote $n trait rows to $OUTPUT_FILE"
}

main "$@"

# Self-test:
#   $ bash scripts/extract_trait_signals.sh <workspace> <output>
# where <workspace>/crates/synthia_fixture contains scripts/fixtures/synthetic_crate
# Expected: 2 trait rows matching the comment block in fixtures/synthetic_crate/src/lib.rs.
#
# Synthetic-drift test:
#   Same setup as self-test, but use fixtures/drift_crate which has a non-pub trait.
#   Expected: 1 row (DriftPubTrait only). NonPubTrait MUST be excluded.
