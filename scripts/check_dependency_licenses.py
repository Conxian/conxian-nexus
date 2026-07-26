#!/usr/bin/env python3
"""Enforce dependency licenses while preserving the known root-license blocker."""

import json
import subprocess
import sys


metadata = json.loads(
    subprocess.check_output(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"])
)
root = next(package for package in metadata["packages"] if package["name"] == "conxian-nexus")
expected = f"{root['name']} = {root['version']} is unlicensed"

command = [
    "cargo", "deny", "--format", "json", "--color", "never", "--locked",
    "check", "licenses", "--hide-inclusion-graph",
]
result = subprocess.run(command, text=True, capture_output=True, check=False)
diagnostics = []
for stream in (result.stdout, result.stderr):
    for line in stream.splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            if line:
                print(line, file=sys.stderr)
            continue
        if item.get("type") == "diagnostic":
            fields = item["fields"]
            diagnostics.append(fields)
            print(f"{fields['severity']}[{fields['code']}]: {fields.get('message', '')}")

errors = [item for item in diagnostics if item.get("severity") == "error"]
if len(errors) != 1 or errors[0].get("code") != "unlicensed" or errors[0].get("message") != expected:
    print("dependency license policy failed; unexpected cargo-deny errors are listed above", file=sys.stderr)
    sys.exit(result.returncode or 1)

print(
    "NOTICE: dependency licenses comply with deny.toml. The repository package remains "
    "unlicensed to Cargo because LICENSE is an incomplete BUSL placeholder; an authorized "
    "owner must supply and approve the complete legal terms before manifest license metadata is added."
)
