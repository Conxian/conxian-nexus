#!/bin/bash
# [CON-411] CI Guardrail for BOS Production Boundary

# Use case-sensitive matching for Stacks addresses and specific placeholders
FORBIDDEN_PATTERNS=(
    "ST[0-9A-Z]{38}"
    "\"SP\.\.\.\""
    "\"ST\.\.\.\""
)

EXCLUDE_DIRS=(
    "node_modules"
    "target"
    ".git"
    "tests"
    "scripts"
    "docs"
    "test-results"
    "playwright-report"
)

EXCLUDE_FILES=(
    "Cargo.lock"
    "CHANGELOG.md"
    "README.md"
    "verify_contamination_guard.py"
    "check_production_boundary.sh"
)

# Build exclude arguments for grep
GREP_EXCLUDES=()
for dir in "${EXCLUDE_DIRS[@]}"; do
    GREP_EXCLUDES+=("--exclude-dir=$dir")
done
for file in "${EXCLUDE_FILES[@]}"; do
    GREP_EXCLUDES+=("--exclude=$file")
done

echo "Starting BOS Production Boundary Check..."

exit_status=0

for pattern in "${FORBIDDEN_PATTERNS[@]}"; do
    echo "Checking for pattern: $pattern"

    # Run grep with the specific pattern using dynamic excludes
    if grep -rE "$pattern" . "${GREP_EXCLUDES[@]}"; then
        echo "FAIL: Forbidden pattern '$pattern' found in production paths."
        exit_status=1
    fi
done

if [ $exit_status -eq 0 ]; then
    echo "PASS: No forbidden patterns found in production-facing paths."
else
    echo "FAILED: Production boundary check failed."
fi

(exit $exit_status)
