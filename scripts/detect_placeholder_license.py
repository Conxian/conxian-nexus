#!/usr/bin/env python3
"""Detect the known incomplete repository license without assessing validity."""

from pathlib import Path
import sys


license_text = Path("LICENSE").read_text(encoding="utf-8")
required_markers = (
    "Change Date:",
    "Change License:",
    "Additional Use Grant:",
    "Business Source License 1.1",
)
placeholder_markers = ("...", "Parameters")

if any(marker not in license_text for marker in required_markers) or all(
    marker in license_text for marker in placeholder_markers
):
    print(
        "ERROR: LICENSE is the incomplete six-line BUSL 1.1 placeholder. An authorized "
        "licensor must provide the complete license text and decide the Change Date, Change "
        "License, and any Additional Use Grant. Do not add Cargo license metadata before that review.",
        file=sys.stderr,
    )
    sys.exit(1)

print(
    "Known six-line BUSL 1.1 placeholder not detected. This check does not assess "
    "license validity, completeness, ownership, or Cargo metadata; obtain legal review."
)
