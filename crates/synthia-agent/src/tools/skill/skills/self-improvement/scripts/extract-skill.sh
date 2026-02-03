#!/bin/bash
# Skill Extraction Helper for Synthia
# Creates a new skill from a learning entry
# Usage: ./extract-skill.sh <skill-name> [--dry-run]

set -e

# Default skills directory (can be overridden)
SKILLS_DIR="./.agents/skills"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

usage() {
    cat << EOF
Usage: $(basename "$0") <skill-name> [options]

Create a new skill from a learning entry.

Arguments:
  skill-name     Name of the skill (lowercase, hyphens for spaces)

Options:
  --dry-run      Show what would be created without creating files
  --output-dir   Relative output directory (default: ./.agents/skills)
  -h, --help     Show this help message

Examples:
  $(basename "$0") docker-m1-fixes
  $(basename "$0") api-timeout-patterns --dry-run
  $(basename "$0") pnpm-setup --output-dir ~/.config/agents/skills

Synthia skill directories (in priority order):
  - ./.agents/skills/      (project-level)
  - ~/.config/agents/skills/ (user-level)
EOF
}

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

# Parse arguments
SKILL_NAME=""
DRY_RUN=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --output-dir)
            if [ -z "${2:-}" ] || [[ "${2:-}" == -* ]]; then
                log_error "--output-dir requires a path argument"
                usage
                exit 1
            fi
            SKILLS_DIR="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            log_error "Unknown option: $1"
            usage
            exit 1
            ;;
        *)
            if [ -z "$SKILL_NAME" ]; then
                SKILL_NAME="$1"
            else
                log_error "Unexpected argument: $1"
                usage
                exit 1
            fi
            shift
            ;;
    esac
done

# Validate skill name
if [ -z "$SKILL_NAME" ]; then
    log_error "Skill name is required"
    usage
    exit 1
fi

# Validate skill name format (lowercase, hyphens, no spaces)
if ! [[ "$SKILL_NAME" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]]; then
    log_error "Invalid skill name format. Use lowercase letters, numbers, and hyphens only."
    log_error "Examples: 'docker-fixes', 'api-patterns', 'pnpm-setup'"
    exit 1
fi

SKILL_PATH="$SKILLS_DIR/$SKILL_NAME"

# Check if skill already exists
if [ -d "$SKILL_PATH" ] && [ "$DRY_RUN" = false ]; then
    log_error "Skill already exists: $SKILL_PATH"
    log_error "Use a different name or remove the existing skill first."
    exit 1
fi

# Dry run output
if [ "$DRY_RUN" = true ]; then
    log_info "Dry run - would create:"
    echo "  $SKILL_PATH/"
    echo "  $SKILL_PATH/SKILL.md"
    echo ""
    echo "Template content:"
    echo "---"
    cat << TEMPLATE
---
name: $SKILL_NAME
description: "[TODO: Add description and trigger conditions]"
---

# $(echo "$SKILL_NAME" | sed 's/-/ /g' | awk '{for(i=1;i<=NF;i++) $i=toupper(substr($i,1,1)) tolower(substr($i,2))}1')

[TODO: Brief introduction]

## Quick Reference

| Situation | Action |
|-----------|--------|
| [Trigger] | [Action] |

## Usage

[TODO: Usage instructions]

## Source

- Learning ID: [TODO]
TEMPLATE
    echo "---"
    exit 0
fi

# Create skill directory structure
log_info "Creating skill: $SKILL_NAME"

mkdir -p "$SKILL_PATH"

# Create SKILL.md from template
cat > "$SKILL_PATH/SKILL.md" << TEMPLATE
---
name: $SKILL_NAME
description: "[TODO: Add description and trigger conditions]"
---

# $(echo "$SKILL_NAME" | sed 's/-/ /g' | awk '{for(i=1;i<=NF;i++) $i=toupper(substr($i,1,1)) tolower(substr($i,2))}1')

[TODO: Brief introduction]

## Quick Reference

| Situation | Action |
|-----------|--------|
| [Trigger] | [Action] |

## Usage

[TODO: Usage instructions]

## Source

- Learning ID: [TODO]
TEMPLATE

log_info "Created: $SKILL_PATH/SKILL.md"

# Suggest next steps
echo ""
log_info "Skill scaffold created!"
echo ""
echo "Next steps:"
echo "  1. Edit $SKILL_PATH/SKILL.md"
echo "  2. Fill in TODO sections from your learning"
echo "  3. Add references/ folder if needed"
echo "  4. Update original learning entry:"
echo "     **Status**: promoted_to_skill"
echo "     **Skill-Path**: $SKILL_PATH"
