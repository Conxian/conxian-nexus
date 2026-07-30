#!/usr/bin/env bash
set -euo pipefail

readonly output_dir="${1:-target/compliance}"
readonly sbom_base="conxian-nexus-sbom"

if ! cargo metadata --locked --format-version 1 | python3 -c '
import json, sys
packages = json.load(sys.stdin)["packages"]
matches = [(p["name"], p["version"]) for p in packages if p["name"] == "xxhash-rust"]
raise SystemExit(0 if matches == [("xxhash-rust", "0.8.15")] else f"unexpected xxhash-rust resolution: {matches}")
'; then
  echo "Refusing to apply the report-only xxhash-rust exception to a changed resolution." >&2
  exit 1
fi

mkdir -p "${output_dir}"
cargo about generate about.hbs --frozen --all-features \
  --output-file "${output_dir}/THIRD_PARTY_LICENSES.html"
grep -q "xxhash-rust 0.8.15" "${output_dir}/THIRD_PARTY_LICENSES.html"
grep -q "lib-conxian-core 0.2.0" "${output_dir}/THIRD_PARTY_LICENSES.html"

rm -f "${sbom_base}.json"
cargo cyclonedx --all-features --format json --spec-version 1.5 \
  --override-filename "${sbom_base}"
python3 scripts/normalize_sbom.py "${sbom_base}.json" "${output_dir}/${sbom_base}.cdx.json"
rm -f "${sbom_base}.json"

python3 - "${output_dir}/${sbom_base}.cdx.json" <<'PY'
import json, pathlib, sys
document = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
if document.get("bomFormat") != "CycloneDX" or document.get("specVersion") != "1.5":
    raise SystemExit("invalid CycloneDX 1.5 SBOM")
if not document.get("components"):
    raise SystemExit("SBOM contains no components")
print(f"Generated SBOM with {len(document['components'])} components")
PY

sha256sum "${output_dir}/THIRD_PARTY_LICENSES.html" "${output_dir}/${sbom_base}.cdx.json"
