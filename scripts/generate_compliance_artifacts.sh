#!/usr/bin/env bash
set -euo pipefail

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/compliance_output_path.sh"

repository_root="$(compliance_repository_root)"
readonly repository_root
output_dir="$(canonical_compliance_output_dir "${repository_root}" "${1:-target/compliance}")"
readonly output_dir
readonly sbom_base="conxian-nexus-sbom"
cd "${repository_root}"

cargo metadata --locked --format-version 1 | python3 -c '
import json, sys
packages = json.load(sys.stdin)["packages"]
expected = {"lib-conxian-core": ("0.3.2", "930caaa839cefb90b5a6c10ae62585e5d893a516"), "xxhash-rust": ("0.8.18", None)}
for name, (version, revision) in expected.items():
    matches = [p for p in packages if p["name"] == name]
    if len(matches) != 1 or matches[0]["version"] != version:
        details = [(p["version"], p.get("source")) for p in matches]
        raise SystemExit(f"unexpected {name} resolution: {details}")
    if revision and revision not in (matches[0].get("source") or ""):
        source = matches[0].get("source")
        raise SystemExit(f"unexpected {name} source: {source}")
'

mkdir -p "${output_dir}"
cargo about generate about.hbs --frozen --all-features --output-file "${output_dir}/THIRD_PARTY_LICENSES.html"
grep -q "xxhash-rust 0.8.18" "${output_dir}/THIRD_PARTY_LICENSES.html"
grep -q "lib-conxian-core 0.3.2" "${output_dir}/THIRD_PARTY_LICENSES.html"

rm -f "${sbom_base}.json"
cargo cyclonedx --all-features --format json --spec-version 1.5 --override-filename "${sbom_base}"
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

test -s "${output_dir}/THIRD_PARTY_LICENSES.html"
test -s "${output_dir}/${sbom_base}.cdx.json"
sha256sum "${output_dir}/THIRD_PARTY_LICENSES.html" "${output_dir}/${sbom_base}.cdx.json"
