#!/usr/bin/env bash
# classify_traits.sh
# Apply decision matrix from design.md §3 to the trait inventory.
# Usage: bash classify_traits.sh [INVENTORY] [OUTPUT]

set -euo pipefail

INV="${1:-/home/crochee/workspace/synthia/openspec/changes/2026-06-15-trait-abstraction-review/artifacts/trait-inventory.md}"
OUT="${2:-/home/crochee/workspace/synthia/openspec/changes/2026-06-15-trait-abstraction-review/artifacts/trait-inventory-classified.md}"

# Preamble: lines 1-3 of inventory (title, generated, workspace)
# Then a blank line
# Then the table header (line 5) and separator (line 6) of the new file
# Then the classified data rows
# Each data row has 10 fields split by '|': trait, file:line, impl, methods,
# generics, lifetimes, assoc_types, call_sites, dyn, body_lines.

{
    # Preamble (3 lines)
    head -n 3 "$INV"
    echo
    # New header with category column appended
    echo "| trait | file:line | impl | methods | generics | lifetimes | assoc_types | call_sites | dyn | body_lines | category |"
    echo "|-------|-----------|------|---------|----------|-----------|-------------|------------|-----|------------|----------|"
    # Classify each data row from original (skip 6-line preamble of source).
    tail -n +7 "$INV" | awk -F'|' '
    function classify(impl, calls, gen, dyn) {
        if (impl == 0) {
            return (dyn == 0) ? "KEEP-dead?" : "KEEP"
        } else if (impl == 1) {
            if (calls < 3 && gen == 0) return "REMOVE_CANDIDATE"
            return "REVIEW"
        } else {
            if (gen >= 2) return "REVIEW"
            return "KEEP"
        }
    }
    {
        # Reconstruct the row without the trailing pipe, then append category and a new pipe.
        # Original row format: | a | b | ... | n |
        # Strip leading and trailing pipes, then re-emit with category column.
        impl  = $4 + 0
        calls = $9 + 0
        gen   = $6 + 0
        dyn   = $10 + 0
        cat   = classify(impl, calls, gen, dyn)
        # Drop first and last empty fields (from leading/trailing |).
        # Then re-build: | a | b | ... | n | cat |
        # The original NF includes leading + 10 data + trailing empty.
        # Field indices: $1=empty (leading |), $2=trait, ..., $11=body_lines, $12=empty (trailing |).
        # Trim leading/trailing whitespace from each field.
        s2=$2; gsub(/^ +| +$/, "", s2)
        s3=$3; gsub(/^ +| +$/, "", s3)
        s4=$4; gsub(/^ +| +$/, "", s4)
        s5=$5; gsub(/^ +| +$/, "", s5)
        s6=$6; gsub(/^ +| +$/, "", s6)
        s7=$7; gsub(/^ +| +$/, "", s7)
        s8=$8; gsub(/^ +| +$/, "", s8)
        s9=$9; gsub(/^ +| +$/, "", s9)
        s10=$10; gsub(/^ +| +$/, "", s10)
        s11=$11; gsub(/^ +| +$/, "", s11)
        printf "| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |\n", \
            s2, s3, s4, s5, s6, s7, s8, s9, s10, s11, cat
    }'
} > "$OUT"

echo "Wrote classified inventory to $OUT"
echo "---"
echo "Category counts:"
echo "  KEEP:             $(grep -c '| KEEP |' "$OUT")"
echo "  KEEP-dead?:       $(grep -c '| KEEP-dead? |' "$OUT")"
echo "  REVIEW:           $(grep -c '| REVIEW |' "$OUT")"
echo "  REMOVE_CANDIDATE: $(grep -c '| REMOVE_CANDIDATE |' "$OUT")"
echo "  TOTAL:            $(grep -c '^| \`' "$OUT")"
