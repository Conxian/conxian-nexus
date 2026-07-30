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
cargo about generate --output-file "${COMPLIANCE_DIR}/licenses.html" about.hbs 2>/dev/null || {
    echo "No about.hbs template found, generating default output"
    cargo about generate --output-file "${COMPLIANCE_DIR}/licenses.txt" 2>&1 || true
}

echo ""
echo "=== SBOM Generation (cargo-cyclonedx) ==="
cargo cyclonedx --format json --output-file "${COMPLIANCE_DIR}/sbom.cdx.json" 2>&1

echo ""
echo "=== License Compliance: PASSED ==="
echo "Artifacts written to ${COMPLIANCE_DIR}/"
ls -la "${COMPLIANCE_DIR}/"
