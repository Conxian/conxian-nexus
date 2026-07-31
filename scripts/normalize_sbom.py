#!/usr/bin/env python3
"""Normalize checkout-specific and generator-time CycloneDX SBOM values."""

import argparse
import json
from pathlib import Path
from urllib.parse import quote

CANONICAL_WORKSPACE_URI = "file:///workspace"


def normalize_string(value: str, workspace_root: Path) -> str:
    root = workspace_root.resolve().as_posix().rstrip("/")
    workspace_uri = workspace_root.resolve().as_uri().rstrip("/")
    replacements = (
        (f"path+{workspace_uri}", f"path+{CANONICAL_WORKSPACE_URI}"),
        (workspace_uri, CANONICAL_WORKSPACE_URI),
        (quote(workspace_uri, safe=""), quote(CANONICAL_WORKSPACE_URI, safe="")),
        (root, "/workspace"),
        (quote(root, safe=""), quote("/workspace", safe="")),
    )
    for original, normalized in replacements:
        value = value.replace(original, normalized)
    return value


def normalize_value(value, workspace_root: Path):
    if isinstance(value, dict):
        return {key: normalize_value(child, workspace_root) for key, child in value.items()}
    if isinstance(value, list):
        return [normalize_value(child, workspace_root) for child in value]
    if isinstance(value, str):
        return normalize_string(value, workspace_root)
    return value


def normalize_document(document: dict, workspace_root: Path) -> dict:
    document.pop("serialNumber", None)
    document.get("metadata", {}).pop("timestamp", None)
    return normalize_value(document, workspace_root)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--workspace-root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    document = json.loads(args.source.read_text(encoding="utf-8"))
    normalized = normalize_document(document, args.workspace_root)
    args.destination.write_text(json.dumps(normalized, indent=2, sort_keys=True) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
