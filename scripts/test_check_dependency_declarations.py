#!/usr/bin/env python3
"""Regression tests for the direct Cargo dependency declaration policy."""

from __future__ import annotations

from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest

CHECKER = Path(__file__).with_name("check_dependency_declarations.py").resolve()
CORE_URL = "https://github.com/Conxian/lib-conxian-core"
CORE_REV = "6075ef7c1640b03246eec4b0d323a19960b18f91"
CORE_SOURCE = f"git+{CORE_URL}?rev={CORE_REV}#{CORE_REV}"
CORE_DECLARATION = f'lib-conxian-core = {{ git = "{CORE_URL}", rev = "{CORE_REV}" }}'


def lock(core_entries: str | None = None) -> str:
    if core_entries is None:
        core_entries = textwrap.dedent(
            f"""
            [[package]]
            name = "lib-conxian-core"
            version = "0.3.2"
            source = "{CORE_SOURCE}"
            """
        )
    return "version = 4\n" + core_entries


def manifest(extra: str = "", core: str = CORE_DECLARATION) -> str:
    return textwrap.dedent(
        f"""
        [package]
        name = "fixture"
        version = "0.1.0"
        edition = "2021"

        [dependencies]
        {core}
        serde = "1"
        {extra}
        """
    )


class CheckerTest(unittest.TestCase):
    def run_fixture(
        self,
        cargo_toml: str,
        cargo_lock: str | None = None,
        files: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text(cargo_toml, encoding="utf-8")
            if cargo_lock is not None:
                (root / "Cargo.lock").write_text(cargo_lock, encoding="utf-8")
            for relative, contents in (files or {}).items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(contents, encoding="utf-8")
            return subprocess.run(
                [sys.executable, str(CHECKER)],
                cwd=root,
                text=True,
                capture_output=True,
                check=False,
            )

    def assert_fails(
        self,
        cargo_toml: str,
        fragment: str,
        cargo_lock: str | None = None,
        files: dict[str, str] | None = None,
    ) -> None:
        result = self.run_fixture(cargo_toml, lock() if cargo_lock is None else cargo_lock, files)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(fragment, result.stderr)

    def test_current_shape_passes(self) -> None:
        result = self.run_fixture(manifest(), lock())
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_valid_workspace_inheritance_passes(self) -> None:
        root = manifest(
            """
            [workspace]
            members = ["member"]

            [workspace.dependencies]
            anyhow = "1"
            """
        )
        member = textwrap.dedent(
            """
            [package]
            name = "member"
            version = "0.1.0"
            edition = "2021"

            [dependencies]
            anyhow = { workspace = true }
            """
        )
        result = self.run_fixture(root, lock(), {"member/Cargo.toml": member})
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_wildcard_and_unversioned_registry_forms_fail(self) -> None:
        cases = {
            'wild = "*"': "wildcard version",
            'wild = "  *  "': "wildcard version",
            'wild = "1.*"': "wildcard version",
            'wild = { version = "1.2.*" }': "wildcard version",
            'wild = { features = ["std"] }': "registry dependency must declare a version",
            'wild = ""': "version must not be empty",
        }
        for declaration, fragment in cases.items():
            with self.subTest(declaration=declaration):
                self.assert_fails(manifest(declaration), fragment)

    def test_path_declarations_fail_in_every_supported_table(self) -> None:
        self.assert_fails(
            manifest('normal = { path = "vendor/normal" }'),
            "[dependencies] dependency 'normal'",
        )
        cases = {
            '[dev-dependencies]\ndev = { path = "vendor/dev" }': "[dev-dependencies] dependency 'dev'",
            '[build-dependencies]\nbuild = { path = "vendor/build" }': "[build-dependencies] dependency 'build'",
            '[target.\'cfg(unix)\'.dependencies]\ntargeted = { path = "vendor/target" }': "target.cfg(unix).dependencies",
            '[target.\'cfg(unix)\'.dev-dependencies]\ntarget-dev = { path = "vendor/target-dev" }': "target.cfg(unix).dev-dependencies",
            '[target.\'cfg(unix)\'.build-dependencies]\ntarget-build = { path = "vendor/target-build" }': "target.cfg(unix).build-dependencies",
            '[workspace.dependencies]\nshared = { path = "vendor/shared" }': "[workspace.dependencies] dependency 'shared'",
        }
        for table, fragment in cases.items():
            with self.subTest(table=table):
                self.assert_fails(manifest() + "\n" + table + "\n", fragment)

    def test_git_and_core_mutations_fail(self) -> None:
        arbitrary = f'arbitrary = {{ git = "https://example.com/repo", rev = "{CORE_REV}" }}'
        core_cases = {
            f'lib-conxian-core = {{ git = "https://example.com/core", rev = "{CORE_REV}" }}': "git must be exactly",
            f'lib-conxian-core = {{ git = "{CORE_URL}", rev = "6187bf6" }}': "rev must be the exact full SHA",
            f'lib-conxian-core = {{ git = "{CORE_URL}", rev = "{CORE_REV[:-1]}0" }}': "rev must be the exact full SHA",
            f'lib-conxian-core = {{ git = "{CORE_URL}", branch = "main" }}': "branch declarations are prohibited",
            f'lib-conxian-core = {{ git = "{CORE_URL}", tag = "v0.3.0" }}': "tag declarations are prohibited",
            f'core-alias = {{ package = "lib-conxian-core", git = "{CORE_URL}", rev = "{CORE_REV}" }}': "renamed, duplicate, or non-root Core",
        }
        self.assert_fails(manifest(arbitrary), "Git dependencies are prohibited")
        for core, fragment in core_cases.items():
            with self.subTest(core=core):
                self.assert_fails(manifest(core="", extra=core), fragment)

        duplicate = manifest() + f"\n[dev-dependencies]\n{CORE_DECLARATION}\n"
        self.assert_fails(duplicate, "renamed, duplicate, or non-root Core")

        member = textwrap.dedent(
            f"""
            [package]
            name = "member"
            version = "0.1.0"
            [dependencies]
            {CORE_DECLARATION}
            """
        )
        self.assert_fails(manifest(), "member/Cargo.toml", files={"member/Cargo.toml": member})

    def test_workspace_override_lock_member_and_config_failures(self) -> None:
        self.assert_fails(manifest("missing = { workspace = true }"), "workspace inheritance source is missing")

        root_wild = manifest(
            """
            inherited = { workspace = true }
            [workspace.dependencies]
            inherited = "1.*"
            """
        )
        self.assert_fails(root_wild, "workspace inheritance source")

        self.assert_fails(manifest() + '\n[patch.crates-io]\nserde = { version = "1" }\n', "[patch]")
        self.assert_fails(manifest() + '\n[replace]\n"serde:1.0.0" = { version = "1.0.1" }\n', "[replace]")

        missing_lock = self.run_fixture(manifest(), cargo_lock=None)
        self.assertNotEqual(missing_lock.returncode, 0)
        self.assertIn("Cargo.lock", missing_lock.stderr)
        self.assert_fails(manifest(), "package array is missing", cargo_lock="")
        self.assert_fails(manifest(), "malformed or unreadable TOML", cargo_lock="not = [toml")
        wrong_version = lock().replace('version = "0.3.2"', 'version = "0.3.0"')
        self.assert_fails(manifest(), "version must be exactly 0.3.2", cargo_lock=wrong_version)
        wrong_source = lock().replace(CORE_SOURCE, "registry+https://github.com/rust-lang/crates.io-index")
        self.assert_fails(manifest(), "source must be exactly", cargo_lock=wrong_source)
        duplicate_lock = lock() + lock().split("version = 4\n", 1)[1]
        self.assert_fails(manifest(), "found 2", cargo_lock=duplicate_lock)
        unrelated_only = lock(
            '[[package]]\nname = "other"\nversion = "0.3.0"\n'
            f'source = "{CORE_SOURCE}"\n'
        )
        self.assert_fails(manifest(), "found 0", cargo_lock=unrelated_only)

        member = "[package]\nname = \"member\"\nversion = \"0.1.0\"\n[dependencies]\nbad = \"*\"\n"
        self.assert_fails(manifest(), "member/Cargo.toml", files={"member/Cargo.toml": member})

        source_config = '[source.crates-io]\nreplace-with = "vendored"\n'
        self.assert_fails(manifest(), "source overrides are prohibited", files={".cargo/config.toml": source_config})
        path_config = 'paths = ["vendor/local"]\n[build]\ntarget-dir = "out"\n'
        self.assert_fails(manifest(), "path overrides are prohibited", files={".cargo/config": path_config})

        harmless_config = '[build]\ntarget-dir = "out"\n[net]\noffline = true\n'
        harmless = self.run_fixture(
            manifest(),
            lock(),
            files={".cargo/config.toml": harmless_config},
        )
        self.assertEqual(harmless.returncode, 0, harmless.stderr)


if __name__ == "__main__":
    unittest.main()
