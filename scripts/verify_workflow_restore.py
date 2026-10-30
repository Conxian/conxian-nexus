#!/usr/bin/env python3
"""Recover real conformance rows on a second disposable PostgreSQL instance.

CI fixtures only. This is not a production backup or arbitrary archive importer.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import time

SCHEMA = re.compile(r"nexus_conformance_[0-9a-f]{32}")
TABLES = {"admin_workflow_entities", "admin_workflow_events"}
FIXTURE_LABEL = "nexus-workflow-recovery"
MAX_ARCHIVE_BYTES = 32 * 1024 * 1024


def validate_container(info: dict, expected_id: str, role: str) -> None:
    config = info.get("Config", {})
    labels = config.get("Labels") or {}
    if (info.get("Id") != expected_id or config.get("Image") != "postgres:17"
            or labels.get("conxian.fixture") != FIXTURE_LABEL
            or labels.get("conxian.fixture.role") != role
            or not info.get("State", {}).get("Running")):
        raise ValueError("expected an explicitly labelled, running PostgreSQL 17 CI fixture")


def validate_snapshot(snapshot: dict) -> None:
    if not snapshot:
        raise ValueError("no actual workflow records available for recovery")
    groups: dict[str, set[str]] = {}
    events = 0
    entities = 0
    for relation, rows in snapshot.items():
        schema, table = relation.split(".", 1)
        if not SCHEMA.fullmatch(schema) or table not in TABLES or not isinstance(rows, list):
            raise ValueError("unexpected workflow snapshot relation")
        groups.setdefault(schema, set()).add(table)
        for row in rows:
            if not isinstance(row, dict):
                raise ValueError("invalid workflow row")
            if table == "admin_workflow_events":
                receipt = json.loads(row["receipt_json"])
                if (not isinstance(receipt, dict) or receipt.get("accepted") is not True
                        or receipt.get("auditEventId") != row["id"]):
                    raise ValueError("persisted audit receipt does not bind its event")
                events += 1
            else:
                entities += 1
    if any(tables != TABLES for tables in groups.values()) or not events or not entities:
        raise ValueError("complete workflow tables with persisted rows and receipts required")


def fingerprint(snapshot: dict) -> str:
    return hashlib.sha256(json.dumps(snapshot, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def verify(source: str, target: str, source_sha: str, output: Path) -> dict:
    if (not re.fullmatch(r"[0-9a-f]{64}", source)
            or not re.fullmatch(r"[0-9a-f]{64}", target) or source == target
            or not re.fullmatch(r"[0-9a-f]{40}", source_sha)):
        raise ValueError("distinct full fixture container IDs and exact source SHA required")
    if output.exists():
        raise ValueError("refusing to overwrite recovery evidence")
    actual_sha = subprocess.check_output(["git", "rev-parse", "HEAD"], timeout=10).decode().strip()
    if actual_sha != source_sha:
        raise ValueError("checkout source does not match the recorded source")
    subprocess.run(["git", "ls-files", "--error-unmatch", "--", "scripts/verify_workflow_restore.py"],
                   check=True, timeout=10, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    subprocess.run(["git", "diff", "--quiet", "HEAD", "--", "scripts/verify_workflow_restore.py"],
                   check=True, timeout=10, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def docker(*args: str, data: bytes | None = None) -> bytes:
        result = subprocess.run(["docker", *args], input=data, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, timeout=60, check=True)
        return result.stdout

    for container, role in [(source, "source"), (target, "restore")]:
        info = json.loads(docker("inspect", container))
        if not isinstance(info, list) or len(info) != 1:
            raise ValueError("unexpected container inspection result")
        validate_container(info[0], container, role)

    def pg(container: str, command: str, *args: str, data: bytes | None = None) -> bytes:
        # Fixed disposable identity; never read DATABASE_URL or inherit libpq
        # overrides from the host/container. No production credentials accepted.
        return docker("exec", "-i", container, "env", "-i", "PATH=/usr/lib/postgresql/17/bin:/usr/local/bin:/usr/bin:/bin",
                      "PGHOST=127.0.0.1", "PGPORT=5432", "PGDATABASE=conformance_test",
                      "PGUSER=conformance", "PGPASSWORD=conformance-fixture-only",
                      "PGCONNECT_TIMEOUT=5", command, *args, data=data)

    def query(container: str, sql: str):
        return json.loads(pg(container, "psql", "-X", "--quiet", "--no-align", "--tuples-only",
                             "--set=ON_ERROR_STOP=1", "--command", sql))

    for container in [source, target]:
        identity = query(container, "SELECT json_build_array(current_database(), current_user, rolsuper) "
                         "FROM pg_roles WHERE rolname = current_user")
        if identity != ["conformance_test", "conformance", False]:
            raise ValueError("disposable fixture database/role identity mismatch")
    # An empty dedicated restore database is mandatory; never clean/drop a
    # supplied target. PostgreSQL catalog, information_schema and public remain.
    schemas = query(target, "SELECT coalesce(json_agg(nspname), '[]') FROM pg_namespace "
                    "WHERE nspname <> 'public' AND nspname <> 'information_schema' AND nspname NOT LIKE 'pg_%'")
    public_tables = query(target, "SELECT coalesce(json_agg(tablename), '[]') FROM pg_tables WHERE schemaname = 'public'")
    if schemas or public_tables:
        raise ValueError("restore fixture must be empty")

    def snapshot(container: str) -> dict:
        relations = query(container, "SELECT coalesce(json_agg(json_build_array(schemaname, tablename) "
                          "ORDER BY schemaname, tablename), '[]') FROM pg_tables "
                          "WHERE schemaname LIKE 'nexus_conformance_%' "
                          "AND tablename IN ('admin_workflow_entities', 'admin_workflow_events')")
        result = {}
        for schema, table in relations:
            if not SCHEMA.fullmatch(schema) or table not in TABLES:
                raise ValueError("unexpected fixture schema/table")
            result[f"{schema}.{table}"] = query(container,
                "SET TIME ZONE 'UTC'; SELECT coalesce(jsonb_agg(row_data ORDER BY row_data::text), '[]'::jsonb) "
                f'FROM (SELECT to_jsonb(t) AS row_data FROM "{schema}"."{table}" t) workflow_rows')
        validate_snapshot(result)
        return result

    before = snapshot(source)
    backup_started = time.monotonic()
    archive = pg(source, "pg_dump", "--format=custom", "--no-owner", "--no-privileges",
                 "--schema=nexus_conformance_*")
    if not 0 < len(archive) <= MAX_ARCHIVE_BYTES:
        raise ValueError("fixture archive exceeds recovery boundary")
    backup_seconds = time.monotonic() - backup_started
    # This deliberately stops only the prevalidated, labelled source fixture.
    # Run after every other test/coverage command that needs that service.
    recovery_started = time.monotonic()
    docker("stop", "--time", "10", source)
    if json.loads(docker("inspect", source))[0]["State"]["Running"]:
        raise ValueError("source fixture did not stop")
    pg(target, "pg_restore", "--dbname=conformance_test", "--single-transaction",
       "--exit-on-error", "--no-owner", "--no-privileges", data=archive)
    after = snapshot(target)
    if before != after:
        raise ValueError("restored workflow records differ from the source")
    if json.loads(docker("inspect", source))[0]["State"]["Running"]:
        raise ValueError("source fixture restarted during recovery verification")
    result = {"schema_version": 1, "source_sha": source_sha,
              "archive_sha256": hashlib.sha256(archive).hexdigest(),
              "workflow_rows_sha256": fingerprint(after), "workflow_tables": len(after),
              "workflow_rows": sum(len(rows) for rows in after.values()),
              "fixture_backup_seconds": round(backup_seconds, 3),
              "fixture_recovery_seconds": round(time.monotonic() - recovery_started, 3),
              "source_instance_stopped": True, "separate_instance_restore_verified": True,
              "fixture_only": True, "operator_independence_verified": False,
              "production_acceptance": False}
    # Publish only measurements/hashes. Test notes, rows and backup bytes are
    # never included in the uploaded receipt and leave with the disposable job.
    with output.open("x") as handle:
        json.dump(result, handle, sort_keys=True, indent=2)
        handle.write("\n")
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-container", required=True)
    parser.add_argument("--restore-container", required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        result = verify(args.source_container, args.restore_container, args.source_sha, args.output)
    except (OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError) as exc:
        parser.exit(1, f"Disposable recovery drill rejected ({type(exc).__name__}).\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
