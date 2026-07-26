#!/usr/bin/env python3
"""Remove generator-time identifiers and serialize a deterministic CycloneDX SBOM."""

import json
from pathlib import Path
import sys


source = Path(sys.argv[1])
destination = Path(sys.argv[2])
document = json.loads(source.read_text(encoding="utf-8"))
document.pop("serialNumber", None)
document.get("metadata", {}).pop("timestamp", None)
destination.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
