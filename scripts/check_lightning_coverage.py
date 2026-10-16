#!/usr/bin/env python3
"""Run and enforce scoped Lightning coverage for Nexus service paths.

This script intentionally enforces coverage on Lightning-focused line ranges
instead of full files because `src/api/rest.rs` and other API files contain
many unrelated endpoints. File-level fail-under would under-report Lightning
work and create noisy regressions for unrelated code.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

# Resolve the intended source blocks on every run. Numeric ranges silently became
# stale when new diagnostic types/handlers moved the billing and REST code.
# Missing, duplicate or reordered anchors must fail instead of measuring other code.
SCOPED_SOURCE_BLOCKS: dict[str, list[tuple[str, str]]] = {
    "src/api/billing/mod.rs": [
        ("pub fn billing_routes()", "\n#[derive(Debug, PartialEq)]\nenum GraceStatus"),
        ("fn determine_grace_status(", "\n#[cfg(test)]\nfn compute_expected_hmac"),
        ("fn validate_telemetry_auth(", "\n/// [NEXUS-01]"),
    ],
    "src/api/billing/nostr.rs": [
        ("const TELEMETRY_EVENT_KIND:", "\npub struct NostrTelemetry"),
    ],
    "src/api/dlc.rs": [
        ("fn validate_dlc_request(", "\n/// [NEXUS-DLC-01]"),
        ("    // 1. Validation", "\n    // 2. Generate DLC Announcement"),
        ("    if payload.dlc_contract_id.trim().is_empty()", "\n    // Compute canonical outcome digest"),
    ],
    "src/api/rest.rs": [
        ('        .nest("/v1/dlc", dlc_routes())', '\n        .nest("/v1/erp", erp_routes())'),
    ],
    "src/executor/lightning.rs": [
        ("impl fmt::Display for LightningFailureType {", "\n#[cfg(test)]\nmod tests"),
    ],
}


def resolve_scoped_ranges(
    source_root: Path,
    blocks: dict[str, list[tuple[str, str]]] = SCOPED_SOURCE_BLOCKS,
) -> dict[str, list[tuple[int, int]]]:
    ranges: dict[str, list[tuple[int, int]]] = {}
    for rel_path, markers in blocks.items():
        source = (source_root / rel_path).read_text()
        ranges[rel_path] = []
        for start, end in markers:
            if source.count(start) != 1 or source.count(end) != 1:
                raise RuntimeError(f"Missing or ambiguous coverage anchors in {rel_path}")
            first, last = source.index(start), source.index(end)
            if last <= first:
                raise RuntimeError(f"Reordered coverage anchors in {rel_path}")
            ranges[rel_path].append(
                (source.count("\n", 0, first) + 1, source.count("\n", 0, last) + 1)
            )
    return ranges


def active_toolchain() -> str:
    output = subprocess.check_output(["rustup", "show", "active-toolchain"], text=True)
    return output.split()[0]


def run_coverage(output_path: Path) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)

    subprocess.run(
        [
            "rustup",
            "component",
            "add",
            "llvm-tools-preview",
            "--toolchain",
            active_toolchain(),
        ],
        check=True,
    )

    subprocess.run(
        [
            "cargo",
            "llvm-cov",
            "--lib",
            "--json",
            "--output-path",
            str(output_path),
        ],
        check=True,
    )


def load_report(path: Path) -> dict:
    if not path.exists():
        raise FileNotFoundError(f"Coverage report missing: {path}")
    return json.loads(path.read_text())


def index_file_entries(report: dict, ranges: dict[str, list[tuple[int, int]]]) -> dict[str, dict]:
    entries = report["data"][0]["files"]
    indexed: dict[str, dict] = {}
    for entry in entries:
        filename = entry["filename"]
        for rel_path in ranges:
            if filename.endswith(rel_path):
                indexed[rel_path] = entry
    return indexed


def line_hit_map(file_entry: dict) -> dict[int, tuple[bool, bool]]:
    # map line -> (instrumented, covered)
    out: dict[int, tuple[bool, bool]] = {}
    for segment in file_entry.get("segments", []):
        line, _col, count, has_count, _is_region_entry, is_gap = segment
        if not has_count or is_gap:
            continue
        inst, cov = out.get(line, (False, False))
        out[line] = (True, cov or count > 0)
    return out


def compute_scoped_coverage(report: dict) -> tuple[list[tuple[str, int, int, float]], int, int, float]:
    scoped_ranges = resolve_scoped_ranges(Path(__file__).resolve().parents[1])
    indexed = index_file_entries(report, scoped_ranges)

    missing_files = [path for path in scoped_ranges if path not in indexed]
    if missing_files:
        raise RuntimeError(f"Scoped files missing from report: {', '.join(missing_files)}")

    rows: list[tuple[str, int, int, float]] = []
    total_covered = 0
    total_instrumented = 0

    for rel_path, ranges in scoped_ranges.items():
        line_map = line_hit_map(indexed[rel_path])
        covered = 0
        instrumented = 0

        for start, end in ranges:
            for line_no in range(start, end + 1):
                inst, cov = line_map.get(line_no, (False, False))
                if inst:
                    instrumented += 1
                    if cov:
                        covered += 1

        if instrumented == 0:
            raise RuntimeError(
                f"No instrumented lines found for scoped ranges in {rel_path}; "
                "ranges may be stale"
            )

        percent = covered / instrumented * 100.0
        rows.append((rel_path, covered, instrumented, percent))
        total_covered += covered
        total_instrumented += instrumented

    aggregate = total_covered / total_instrumented * 100.0 if total_instrumented else 0.0
    return rows, total_covered, total_instrumented, aggregate


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-path",
        default="target/llvm-cov/lightning-scoped.json",
        help="Path for llvm-cov JSON export",
    )
    parser.add_argument(
        "--min-percent",
        type=float,
        default=90.0,
        help="Minimum aggregate line coverage required",
    )
    parser.add_argument(
        "--skip-run",
        action="store_true",
        help="Skip running llvm-cov and only evaluate existing JSON",
    )
    args = parser.parse_args()

    output_path = Path(args.output_path)

    try:
        if not args.skip_run:
            run_coverage(output_path)

        report = load_report(output_path)
        rows, covered, instrumented, aggregate = compute_scoped_coverage(report)
    except Exception as exc:  # pragma: no cover
        print(f"[lightning-coverage] ERROR: {exc}", file=sys.stderr)
        return 2

    print("[lightning-coverage] scoped line coverage")
    for rel_path, row_covered, row_instrumented, row_percent in rows:
        print(
            f"- {rel_path}: {row_covered}/{row_instrumented} "
            f"({row_percent:.2f}%)"
        )

    print(
        f"[lightning-coverage] aggregate: {covered}/{instrumented} "
        f"({aggregate:.2f}%), required >= {args.min_percent:.2f}%"
    )

    if aggregate < args.min_percent:
        print("[lightning-coverage] FAIL: coverage below threshold", file=sys.stderr)
        return 1

    print("[lightning-coverage] PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
