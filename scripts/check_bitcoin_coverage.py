#!/usr/bin/env python3
"""Run and enforce scoped Bitcoin coverage for Nexus DLC/MMR/RGB paths.

This script intentionally enforces coverage on Bitcoin-focused line ranges
instead of full files because mixed modules (especially `src/api/rest.rs`)
contain many unrelated endpoints. File-level fail-under would under-report
Bitcoin work and introduce noisy regressions from unrelated areas.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

# Bitcoin-focused ranges (inclusive line numbers).
# Keep these in sync with source edits that move logic blocks.
SCOPED_LINE_RANGES: dict[str, list[tuple[int, int]]] = {
    "src/api/dlc.rs": [
        (25, 54),  # request validation + announcement/signing helpers
        (65, 76),  # deterministic invalid-request branch
    ],
    "src/api/rest.rs": [
        (133, 146), # app_router wiring (Bitcoin-focused routes)
        (230, 270), # get_mmr_proof handler
        (432, 457), # get_rgb_contract handler
        (458, 500), # get_event_feed handler
        (625, 711), # MMR proof tests
        (712, 815), # RGB contract tests
        (861, 877), # Event feed tests
    ],
    "src/executor/rgb.rs": [
        (38, 45),   # rollout mode display
        (104, 140), # lookup validation + mode branches
    ],
}


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


def index_file_entries(report: dict) -> dict[str, dict]:
    entries = report["data"][0]["files"]
    indexed: dict[str, dict] = {}
    for entry in entries:
        filename = entry["filename"]
        for rel_path in SCOPED_LINE_RANGES:
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


def compute_scoped_coverage(
    report: dict,
) -> tuple[list[tuple[str, int, int, float]], int, int, float]:
    indexed = index_file_entries(report)

    missing_files = [path for path in SCOPED_LINE_RANGES if path not in indexed]
    if missing_files:
        raise RuntimeError(f"Scoped files missing from report: {', '.join(missing_files)}")

    rows: list[tuple[str, int, int, float]] = []
    total_covered = 0
    total_instrumented = 0

    for rel_path, ranges in SCOPED_LINE_RANGES.items():
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


def write_summary(
    summary_path: Path,
    rows: list[tuple[str, int, int, float]],
    covered: int,
    instrumented: int,
    aggregate: float,
    required_percent: float,
    report_path: Path,
) -> None:
    summary_path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "coverageScope": "bitcoin-scoped-lines",
        "requiredPercent": required_percent,
        "aggregatePercent": aggregate,
        "totalCovered": covered,
        "totalInstrumented": instrumented,
        "passed": aggregate >= required_percent,
        "reportPath": str(report_path),
        "files": [
            {
                "path": rel_path,
                "covered": row_covered,
                "instrumented": row_instrumented,
                "percent": row_percent,
            }
            for rel_path, row_covered, row_instrumented, row_percent in rows
        ],
    }
    summary_path.write_text(json.dumps(payload, indent=2) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-path",
        default="target/llvm-cov/bitcoin-scoped.json",
        help="Path for llvm-cov JSON export",
    )
    parser.add_argument(
        "--summary-path",
        default=None,
        help="Optional path for machine-readable scoped summary JSON",
    )
    parser.add_argument(
        "--min-percent",
        type=float,
        default=95.0,
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
        if args.summary_path:
            write_summary(
                Path(args.summary_path),
                rows,
                covered,
                instrumented,
                aggregate,
                args.min_percent,
                output_path,
            )
    except Exception as exc:  # pragma: no cover
        print(f"[bitcoin-coverage] ERROR: {exc}", file=sys.stderr)
        return 2

    print("[bitcoin-coverage] scoped line coverage")
    for rel_path, row_covered, row_instrumented, row_percent in rows:
        print(
            f"- {rel_path}: {row_covered}/{row_instrumented} "
            f"({row_percent:.2f}%)"
        )

    print(
        f"[bitcoin-coverage] aggregate: {covered}/{instrumented} "
        f"({aggregate:.2f}%), required >= {args.min_percent:.2f}%"
    )

    if args.summary_path:
        print(f"[bitcoin-coverage] summary json: {args.summary_path}")

    if aggregate < args.min_percent:
        print("[bitcoin-coverage] FAIL: coverage below threshold", file=sys.stderr)
        return 1

    print("[bitcoin-coverage] PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
