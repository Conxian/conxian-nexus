#!/usr/bin/env python3
"""Enforce fail-closed policy for direct Cargo dependency declarations."""

from __future__ import annotations

import os
from pathlib import Path
import sys
import tomllib
from typing import Any

CORE_NAME = "lib-conxian-core"
CORE_VERSION = "0.3.1"
CORE_URL = "https://github.com/Conxian/lib-conxian-core"
CORE_REV = "d9e0f3a2fd0c854ab833ca4831c1f6e3e275cb5b"
CORE_LOCK_SOURCE = (
    "git+https://github.com/Conxian/lib-conxian-core"
    f"?rev={CORE_REV}#{CORE_REV}"
)

DEPENDENCY_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")
SOURCE_KEYS = ("git", "path", "branch", "tag", "registry")


def load_toml(path: Path, errors: list[str]) -> dict[str, Any] | None:
    try:
        with path.open("rb") as handle:
            value = tomllib.load(handle)
    except (OSError, tomllib.TOMLDecodeError) as exc:
        errors.append(f"{path.as_posix()}: malformed or unreadable TOML: {exc}")
        return None
    if not isinstance(value, dict):
        errors.append(f"{path.as_posix()}: TOML root must be a table")
        return None
    return value


def repository_files(root: Path, filename: str) -> list[Path]:
    matches: list[Path] = []
    for directory, names, files in os.walk(root):
        names[:] = sorted(name for name in names if name not in {".git", "target"})
        if filename in files:
            matches.append(Path(directory) / filename)
    return sorted(matches, key=lambda path: path.relative_to(root).as_posix())


def dependency_tables(manifest: dict[str, Any]) -> list[tuple[str, dict[str, Any]]]:
    tables: list[tuple[str, dict[str, Any]]] = []
    for table_name in DEPENDENCY_TABLES:
        table = manifest.get(table_name)
        if isinstance(table, dict):
            tables.append((table_name, table))

    workspace = manifest.get("workspace")
    if isinstance(workspace, dict) and isinstance(workspace.get("dependencies"), dict):
        tables.append(("workspace.dependencies", workspace["dependencies"]))

    target = manifest.get("target")
    if isinstance(target, dict):
        for target_name in sorted(target):
            target_config = target[target_name]
            if not isinstance(target_config, dict):
                continue
            for table_name in DEPENDENCY_TABLES:
                table = target_config.get(table_name)
                if isinstance(table, dict):
                    tables.append((f"target.{target_name}.{table_name}", table))
    return tables


def context(manifest_path: Path, table_name: str, dependency: str) -> str:
    return f"{manifest_path.as_posix()} [{table_name}] dependency {dependency!r}"


def declaration_errors(spec: Any) -> list[str]:
    errors: list[str] = []
    if isinstance(spec, str):
        version = spec.strip()
        if not version:
            errors.append("version must not be empty")
        elif "*" in version:
            errors.append(f"wildcard version is prohibited: {spec!r}")
        return errors

    if not isinstance(spec, dict):
        return ["declaration must be a version string or inline table"]

    for key in ("path", "branch", "tag"):
        if key in spec:
            errors.append(f"{key} declarations are prohibited")

    inherited = spec.get("workspace") is True
    if "workspace" in spec and not inherited:
        errors.append("workspace inheritance must be literal true")

    version = spec.get("version")
    if version is not None:
        if not isinstance(version, str) or not version.strip():
            errors.append("version must be a non-empty string")
        elif "*" in version:
            errors.append(f"wildcard version is prohibited: {version!r}")

    if inherited:
        conflicting = sorted(key for key in (*SOURCE_KEYS, "version") if key in spec)
        if conflicting:
            errors.append(
                "workspace inheritance cannot declare source/version keys: "
                + ", ".join(conflicting)
            )
    elif "git" not in spec and "path" not in spec and version is None:
        errors.append("registry dependency must declare a version")

    return errors


def validate_repository(root: Path) -> list[str]:
    errors: list[str] = []
    root_manifest_path = root / "Cargo.toml"
    manifests = repository_files(root, "Cargo.toml")
    if root_manifest_path not in manifests:
        errors.append("Cargo.toml: root manifest is missing")

    parsed: dict[Path, dict[str, Any]] = {}
    for manifest_path in manifests:
        relative = manifest_path.relative_to(root)
        manifest = load_toml(relative, errors)
        if manifest is not None:
            parsed[relative] = manifest

    root_manifest = parsed.get(Path("Cargo.toml"))
    workspace_dependencies: dict[str, Any] = {}
    if root_manifest is not None:
        workspace = root_manifest.get("workspace")
        if isinstance(workspace, dict) and isinstance(workspace.get("dependencies"), dict):
            workspace_dependencies = workspace["dependencies"]

        for override in ("patch", "replace"):
            table = root_manifest.get(override)
            if isinstance(table, dict) and table:
                errors.append(f"Cargo.toml [{override}]: source overrides are prohibited")

    valid_core_declarations = 0
    for manifest_path in sorted(parsed, key=lambda path: path.as_posix()):
        manifest = parsed[manifest_path]
        for table_name, table in dependency_tables(manifest):
            for dependency in sorted(table):
                spec = table[dependency]
                declaration_context = context(manifest_path, table_name, dependency)
                for error in declaration_errors(spec):
                    errors.append(f"{declaration_context}: {error}")

                package_name = spec.get("package") if isinstance(spec, dict) else None
                is_core = dependency == CORE_NAME or package_name == CORE_NAME
                is_allowed_core_location = (
                    manifest_path == Path("Cargo.toml")
                    and table_name == "dependencies"
                    and dependency == CORE_NAME
                    and package_name is None
                )

                if isinstance(spec, dict) and spec.get("workspace") is True:
                    inherited_spec = workspace_dependencies.get(dependency)
                    if inherited_spec is None:
                        errors.append(
                            f"{declaration_context}: workspace inheritance source is missing "
                            "from Cargo.toml [workspace.dependencies]"
                        )
                    elif declaration_errors(inherited_spec):
                        errors.append(
                            f"{declaration_context}: workspace inheritance source in "
                            "Cargo.toml [workspace.dependencies] is invalid"
                        )

                if is_core and not is_allowed_core_location:
                    errors.append(
                        f"{declaration_context}: renamed, duplicate, or non-root Core "
                        "declaration is prohibited"
                    )

                if isinstance(spec, dict) and "git" in spec:
                    if not is_allowed_core_location:
                        errors.append(
                            f"{declaration_context}: Git dependencies are prohibited except "
                            "for the exact root Core declaration"
                        )
                    else:
                        core_errors: list[str] = []
                        if spec.get("git") != CORE_URL:
                            core_errors.append(f"git must be exactly {CORE_URL}")
                        if spec.get("rev") != CORE_REV:
                            core_errors.append(f"rev must be the exact full SHA {CORE_REV}")
                        if any(key in spec for key in ("package", "path", "branch", "tag")):
                            core_errors.append("package/path/branch/tag keys are prohibited")
                        if "version" in spec and spec.get("version") != CORE_VERSION:
                            core_errors.append(f"version, when present, must be {CORE_VERSION}")
                        if core_errors:
                            for error in core_errors:
                                errors.append(f"{declaration_context}: {error}")
                        else:
                            valid_core_declarations += 1

    if valid_core_declarations != 1:
        errors.append(
            "Cargo.toml [dependencies]: exactly one valid literal unrenamed "
            f"{CORE_NAME} Git declaration is required"
        )

    for config_name in ("config", "config.toml"):
        for config_path in repository_files(root, config_name):
            if config_path.parent.name != ".cargo":
                continue
            relative = config_path.relative_to(root)
            config = load_toml(relative, errors)
            if config is None:
                continue
            source = config.get("source")
            if isinstance(source, dict) and source:
                errors.append(f"{relative.as_posix()} [source]: source overrides are prohibited")
            paths = config.get("paths")
            if paths not in (None, [], {}):
                errors.append(f"{relative.as_posix()} paths: path overrides are prohibited")

    lock_path = Path("Cargo.lock")
    lock = load_toml(lock_path, errors)
    if lock is not None:
        packages = lock.get("package")
        if not isinstance(packages, list):
            errors.append("Cargo.lock: package array is missing or malformed")
        else:
            core_packages = [
                package
                for package in packages
                if isinstance(package, dict) and package.get("name") == CORE_NAME
            ]
            if len(core_packages) != 1:
                errors.append(
                    f"Cargo.lock: expected exactly one {CORE_NAME} package, "
                    f"found {len(core_packages)}"
                )
            else:
                package = core_packages[0]
                if package.get("version") != CORE_VERSION:
                    errors.append(
                        f"Cargo.lock: {CORE_NAME} version must be exactly {CORE_VERSION}"
                    )
                if package.get("source") != CORE_LOCK_SOURCE:
                    errors.append(
                        f"Cargo.lock: {CORE_NAME} source must be exactly {CORE_LOCK_SOURCE}"
                    )

    return sorted(set(errors))


def main() -> int:
    errors = validate_repository(Path.cwd())
    if errors:
        print("Dependency declaration policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Dependency declarations and locked Core source satisfy the direct-source policy.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
