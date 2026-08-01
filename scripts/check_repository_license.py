#!/usr/bin/env python3
"""Mechanically validate LICENSE structure without judging legal authority."""

from pathlib import Path
import sys

license_text = Path("LICENSE").read_text(encoding="utf-8")
required_markers = (
    "Business Source License 1.1",
    "Licensor:",
    "Licensed Work:",
    "Additional Use Grant:",
    "Change Date:",
    "Change License:",
    "Terms",
    "Covenants of Licensor",
)
errors = [f"missing required marker: {marker}" for marker in required_markers if marker not in license_text]
if len(license_text.splitlines()) < 60:
    errors.append("license text is unexpectedly short")
if "..." in license_text:
    errors.append("license text contains an ellipsis placeholder")

if errors:
    print("Repository license structure check failed:", file=sys.stderr)
    for error in errors:
        print(f"- {error}", file=sys.stderr)
    sys.exit(1)

print("LICENSE has expected structural markers; this check does not approve legal terms or authority.")
