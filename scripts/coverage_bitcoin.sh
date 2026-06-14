#!/usr/bin/env bash
set -euo pipefail

# Scoped Bitcoin coverage wrapper for CI
# Uses check_bitcoin_coverage.py and emits artifacts for GitHub Step Summary

MIN_PERCENT="${1:-95}"
OUTPUT_DIR="target/coverage/bitcoin"
SUMMARY_JSON="$OUTPUT_DIR/summary.json"
SUMMARY_MD="$OUTPUT_DIR/summary.md"

mkdir -p "$OUTPUT_DIR"

python3 scripts/check_bitcoin_coverage.py --min-percent "$MIN_PERCENT" --summary-path "$SUMMARY_JSON"

# Generate markdown summary
python3 - "$SUMMARY_JSON" "$SUMMARY_MD" <<'PY'
import json, sys, pathlib
summary = json.loads(pathlib.Path(sys.argv[1]).read_text())
lines = [
    "### Bitcoin Scoped Coverage Summary",
    f"- Status: **{'PASS' if summary['passed'] else 'FAIL'}**",
    f"- Aggregate Coverage: **{summary['aggregatePercent']:.2f}%**",
    f"- Required Threshold: **{summary['requiredPercent']:.2f}%**",
    "",
    "| File | Coverage |",
    "| :--- | :--- |"
]
for f in summary['files']:
    lines.append(f"| `{f['path']}` | {f['percent']:.2f}% |")
pathlib.Path(sys.argv[2]).write_text("\n".join(lines))
PY

echo "Bitcoin coverage artifacts generated in $OUTPUT_DIR"
