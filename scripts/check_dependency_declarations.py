#!/usr/bin/env python3
"""Reject wildcard versions and unpinned git dependencies in Cargo.toml."""

import re
from pathlib import Path
import sys
import tomllib


manifest = tomllib.loads(Path("Cargo.toml").read_text(encoding="utf-8"))
errors = []


def inspect_table(table, location):
    for name, spec in table.items():
        if isinstance(spec, str):
            if spec.strip() == "*":
                errors.append(f"{location}.{name}: wildcard version")
            continue
        if not isinstance(spec, dict):
            continue
        if spec.get("version", "").strip() == "*":
            errors.append(f"{location}.{name}: wildcard version")
        if "git" in spec and not re.fullmatch(r"[0-9a-fA-F]{40}", str(spec.get("rev", ""))):
            errors.append(f"{location}.{name}: git dependency must use a full 40-character rev")


def walk(value, location="Cargo.toml"):
    if not isinstance(value, dict):
        return
    for key, child in value.items():
        child_location = f"{location}.{key}"
        if key in {"dependencies", "dev-dependencies", "build-dependencies"} and isinstance(child, dict):
            inspect_table(child, child_location)
        walk(child, child_location)


walk(manifest)
if errors:
    print("Dependency declaration policy failed:", file=sys.stderr)
    for error in errors:
        print(f"- {error}", file=sys.stderr)
    sys.exit(1)

print("Dependency declarations use constrained versions and revision-pinned git sources.")
