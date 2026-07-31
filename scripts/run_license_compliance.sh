#!/usr/bin/env bash
set -euo pipefail

readonly output_dir="${1:-target/compliance}"
mkdir -p target
comparison_dir="$(mktemp -d target/compliance-compare.XXXXXX)"
readonly comparison_dir
trap 'rm -rf "${comparison_dir}"' EXIT

python3 scripts/test_check_dependency_declarations.py
python3 scripts/check_dependency_declarations.py
python3 scripts/check_repository_license.py
cargo deny --locked check licenses bans sources --hide-inclusion-graph
python3 scripts/test_normalize_sbom.py

rm -rf "${output_dir}"
scripts/generate_compliance_artifacts.sh "${output_dir}"
scripts/generate_compliance_artifacts.sh "${comparison_dir}"
cmp "${output_dir}/THIRD_PARTY_LICENSES.html" "${comparison_dir}/THIRD_PARTY_LICENSES.html"
cmp "${output_dir}/conxian-nexus-sbom.cdx.json" "${comparison_dir}/conxian-nexus-sbom.cdx.json"
test -s "${output_dir}/THIRD_PARTY_LICENSES.html"
test -s "${output_dir}/conxian-nexus-sbom.cdx.json"
echo "Compliance artifacts are non-empty and deterministic across consecutive runs."
