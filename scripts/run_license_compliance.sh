#!/usr/bin/env bash
set -euo pipefail

# License compliance script for Conxian Nexus
# Runs cargo-deny for license policy enforcement,
# cargo-about for license attribution, and
# cargo-cyclonedx for SBOM generation.
#
# SPDX-License-Identifier: BUSL-1.1

COMPLIANCE_DIR="target/compliance"
mkdir -p "${COMPLIANCE_DIR}"

echo "=== License Policy Check (cargo-deny) ==="
cargo deny check licenses 2>&1 | tee "${COMPLIANCE_DIR}/deny-licenses.log"

echo ""
echo "=== License Attribution (cargo-about) ==="
cargo about generate --locked --output-file "${COMPLIANCE_DIR}/licenses.html" about.hbs

echo ""
echo "=== SBOM Generation (cargo-cyclonedx) ==="
cargo cyclonedx --format json --override-filename sbom.cdx 2>&1
mv sbom.cdx.json "${COMPLIANCE_DIR}/sbom.cdx.json"

echo ""
echo "=== License Compliance: PASSED ==="
echo "Artifacts written to ${COMPLIANCE_DIR}/"
ls -la "${COMPLIANCE_DIR}/"
