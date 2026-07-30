#!/usr/bin/env bash
set -euo pipefail

readonly output_dir="${1:-target/compliance}"

python3 scripts/check_dependency_licenses.py
python3 scripts/check_dependency_declarations.py
cargo deny --locked check bans sources --hide-inclusion-graph
python3 scripts/test_normalize_sbom.py
scripts/generate_compliance_artifacts.sh "${output_dir}"
