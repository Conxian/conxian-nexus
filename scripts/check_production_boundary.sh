#!/bin/bash
# [CON-411] CI Guardrail for BOS Production Boundary

# Use case-sensitive matching for Stacks addresses and specific placeholders
# "SP\.\.\." matches the literal string SP...
FORBIDDEN_PATTERNS=(
    "ST[0-9A-Z]{38}"
    "\"SP\.\.\.\""
    "\"ST\.\.\.\""
)

EXCLUDE_PATHS=(
    "node_modules"
    "target"
    ".git"
    "Cargo.lock"
    "tests"
    "scripts"
    "CHANGELOG.md"
    "README.md"
    "docs"
)

echo "Starting BOS Production Boundary Check..."

exit_status=0

for pattern in "${FORBIDDEN_PATTERNS[@]}"; do
    echo "Checking for pattern: $pattern"

    # Run grep with the specific pattern
    # We use -E for extended regex
    if grep -rE "$pattern" . --exclude-dir=node_modules --exclude-dir=target --exclude-dir=.git --exclude-dir=tests --exclude-dir=scripts --exclude-dir=docs --exclude=Cargo.lock --exclude=CHANGELOG.md --exclude=README.md | grep -v "check_production_boundary.sh"; then
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
