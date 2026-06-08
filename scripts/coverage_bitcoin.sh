#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/coverage_bitcoin.sh [--min-percent <value>] [--output-dir <path>]

Runs scoped Bitcoin coverage checks and emits machine-readable artifacts:
- <output-dir>/llvm-cov-bitcoin.json
- <output-dir>/summary.json
- <output-dir>/summary.md
USAGE
}

MIN_PERCENT="${BITCOIN_COVERAGE_MIN_PERCENT:-95}"
OUTPUT_DIR="${BITCOIN_COVERAGE_OUTPUT_DIR:-target/coverage/bitcoin}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --min-percent)
      MIN_PERCENT="$2"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

LLVM_JSON_PATH="$OUTPUT_DIR/llvm-cov-bitcoin.json"
SUMMARY_JSON_PATH="$OUTPUT_DIR/summary.json"
SUMMARY_MD_PATH="$OUTPUT_DIR/summary.md"

mkdir -p "$OUTPUT_DIR"

python3 scripts/check_bitcoin_coverage.py \
  --min-percent "$MIN_PERCENT" \
  --output-path "$LLVM_JSON_PATH" \
  --summary-path "$SUMMARY_JSON_PATH"

python3 - "$SUMMARY_JSON_PATH" "$SUMMARY_MD_PATH" <<'PY'
import json
import pathlib
import sys

summary_path = pathlib.Path(sys.argv[1])
markdown_path = pathlib.Path(sys.argv[2])
summary = json.loads(summary_path.read_text())

lines = [
    "### Bitcoin scoped coverage",
    "",
    f"- Required threshold: **{summary['requiredPercent']:.2f}%**",
    (
        f"- Aggregate: **{summary['aggregatePercent']:.2f}%** "
        f"({summary['totalCovered']}/{summary['totalInstrumented']})"
    ),
    f"- Result: **{'PASS' if summary['passed'] else 'FAIL'}**",
    "",
    "| File | Covered | Instrumented | Percent |",
    "| --- | ---: | ---: | ---: |",
]

for row in summary["files"]:
    lines.append(
        f"| `{row['path']}` | {row['covered']} | {row['instrumented']} | {row['percent']:.2f}% |"
    )

markdown_path.write_text("\n".join(lines) + "\n")
PY

echo "[bitcoin-coverage] artifacts"
echo "- llvm-cov json: $LLVM_JSON_PATH"
echo "- summary json:  $SUMMARY_JSON_PATH"
echo "- summary md:    $SUMMARY_MD_PATH"
