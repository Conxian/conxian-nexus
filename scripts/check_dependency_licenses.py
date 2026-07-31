#!/usr/bin/env python3
"""Enforce dependency licenses without masking policy failures."""

import subprocess
import sys


command = [
    "cargo", "deny", "--color", "never", "--locked",
    "check", "licenses", "--hide-inclusion-graph",
]
result = subprocess.run(command, check=False)
if result.returncode:
    print("dependency license policy failed; cargo-deny errors are listed above", file=sys.stderr)
    sys.exit(result.returncode)

print("Dependency licenses comply with deny.toml.")
