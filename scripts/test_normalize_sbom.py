#!/usr/bin/env python3
"""Test checkout-independent, reference-consistent SBOM output."""

import copy
import hashlib
import json
from pathlib import Path
import unittest

from normalize_sbom import normalize_document


def fixture(root: Path) -> dict:
    root_uri = root.as_uri()
    root_ref = f"path+{root_uri}#0.4.22"
    target_ref = f"{root_ref} bin-target-0"
    return {
        "serialNumber": "urn:uuid:checkout-specific",
        "metadata": {"timestamp": "2026-07-26T00:00:00Z", "component": {
            "bom-ref": root_ref,
            "purl": f"pkg:cargo/conxian-nexus@0.4.22?download_url={root_uri}",
            "components": [{"bom-ref": target_ref, "path": str(root / "src/main.rs")}],
        }},
        "dependencies": [{"ref": root_ref, "dependsOn": [target_ref]}],
    }


class NormalizeSbomTests(unittest.TestCase):
    def test_distinct_checkout_prefixes_have_identical_references_and_hashes(self):
        roots = [Path("/tmp/checkout-a/conxian-nexus"), Path("/opt/checkout-b/conxian-nexus")]
        outputs = [normalize_document(copy.deepcopy(fixture(root)), root) for root in roots]
        serialized = [json.dumps(item, sort_keys=True).encode() for item in outputs]
        self.assertEqual(outputs[0], outputs[1])
        self.assertEqual(hashlib.sha256(serialized[0]).digest(), hashlib.sha256(serialized[1]).digest())
        root_ref = outputs[0]["metadata"]["component"]["bom-ref"]
        self.assertEqual(root_ref, outputs[0]["dependencies"][0]["ref"])
        self.assertEqual(outputs[0]["metadata"]["component"]["components"][0]["bom-ref"], outputs[0]["dependencies"][0]["dependsOn"][0])
        self.assertNotIn(str(roots[0]), serialized[0].decode())
        self.assertNotIn(str(roots[1]), serialized[1].decode())


if __name__ == "__main__":
    unittest.main()
