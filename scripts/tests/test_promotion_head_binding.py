"""Regression and process tests for immutable promotion candidates."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/branch_promotion_policy.py"
spec = importlib.util.spec_from_file_location("promotion_policy_under_test", SCRIPT)
assert spec and spec.loader
policy = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = policy
spec.loader.exec_module(policy)

SOURCE = "a" * 40
TARGET = "b" * 40
OTHER = "c" * 40
REPO = "Conxian/policy-test"
STAGED = """### Dev -> staged promotion checklist
- [x] wallet, signer, treasury, deployment boundary
"""
MAIN = """### Mainnet acceptance evidence pack
#### Promotion metadata
Synthetic policy fixture.
#### Mainnet-only production scope
Synthetic policy fixture.
#### Contamination and residue proof
Synthetic policy fixture.
#### Successful production validation
Synthetic policy fixture.
#### Release-readiness sign-off
Synthetic policy fixture.
#### Owner accountability
Synthetic policy fixture.
"""
EVIDENCE = (
    f"- Promotion source SHA: `{SOURCE}`\n"
    f"- Promotion target-base SHA: `{TARGET}`\n"
    f"- Promotion commit window: `{TARGET}..{SOURCE}`\n"
)
LANES = (("dev-to-staged", "staged", STAGED), ("staged-to-main", "main", MAIN))


def event(lane, *, head_sha=SOURCE):
    prefix, base, body = lane
    return {
        "pull_request": {
            "number": 100,
            "base": {"ref": base, "sha": TARGET, "repo": {"full_name": REPO}},
            "head": {
                "ref": f"promotion/{prefix}-{SOURCE}",
                "sha": head_sha,
                "repo": {"full_name": REPO},
            },
            "body": body + EVIDENCE,
            "user": {"login": "contributor"},
        }
    }


def errors(data):
    return policy.validate_pull_request(
        policy.context_from_event(data), policy.BootstrapException(0)
    )


class PromotionHeadBindingTests(unittest.TestCase):
    def test_exact_candidate_in_both_directions(self):
        for lane in LANES:
            with self.subTest(lane=lane[0]):
                self.assertEqual([], errors(event(lane)))

    def test_actual_head_must_match_encoded_source(self):
        for lane in LANES:
            with self.subTest(lane=lane[0]):
                found = errors(event(lane, head_sha=OTHER))
                self.assertTrue(any("head SHA" in item for item in found), found)

    def test_absent_head_is_rejected(self):
        for lane in LANES:
            data = event(lane)
            del data["pull_request"]["head"]["sha"]
            with self.subTest(lane=lane[0]):
                self.assertTrue(errors(data))

    def test_malformed_head_is_rejected_without_exception(self):
        for lane in LANES:
            for value in (None, "", "a" * 39, "a" * 41, "g" * 40, "A" * 40, 0, [], {}):
                with self.subTest(lane=lane[0], value=value):
                    self.assertTrue(errors(event(lane, head_sha=value)))

    def test_stale_source_metadata_is_rejected(self):
        for lane in LANES:
            data = event(lane)
            data["pull_request"]["body"] = data["pull_request"]["body"].replace(
                f"source SHA: `{SOURCE}`", f"source SHA: `{OTHER}`"
            )
            with self.subTest(lane=lane[0]):
                self.assertTrue(errors(data))

    def test_stale_window_end_is_rejected(self):
        for lane in LANES:
            data = event(lane)
            data["pull_request"]["body"] = data["pull_request"]["body"].replace(
                f"{TARGET}..{SOURCE}", f"{TARGET}..{OTHER}"
            )
            with self.subTest(lane=lane[0]):
                self.assertTrue(errors(data))

    def test_stale_target_base_is_rejected(self):
        for lane in LANES:
            data = event(lane)
            data["pull_request"]["base"]["sha"] = OTHER
            with self.subTest(lane=lane[0]):
                self.assertTrue(errors(data))

    def test_missing_or_malformed_base_is_rejected_without_exception(self):
        for lane in LANES:
            for value in (None, "", "b" * 39, 0, [], {}):
                data = event(lane)
                data["pull_request"]["base"]["sha"] = value
                with self.subTest(lane=lane[0], value=value):
                    self.assertTrue(errors(data))

    def test_window_start_must_match_recorded_base(self):
        for lane in LANES:
            data = event(lane)
            data["pull_request"]["body"] = data["pull_request"]["body"].replace(
                f"{TARGET}..{SOURCE}", f"{OTHER}..{SOURCE}"
            )
            with self.subTest(lane=lane[0]):
                self.assertTrue(errors(data))

    def test_moving_head_requires_a_new_candidate(self):
        for lane in LANES:
            data = event(lane)
            self.assertEqual([], errors(data))
            data["pull_request"]["head"]["sha"] = OTHER
            self.assertTrue(errors(data))
            prefix, _, _ = lane
            data["pull_request"]["head"]["ref"] = f"promotion/{prefix}-{OTHER}"
            data["pull_request"]["body"] = data["pull_request"]["body"].replace(SOURCE, OTHER)
            self.assertEqual([], errors(data))

    def test_forked_candidate_is_still_rejected(self):
        for lane in LANES:
            data = event(lane)
            data["pull_request"]["head"]["repo"]["full_name"] = "external/fork"
            with self.subTest(lane=lane[0]):
                self.assertTrue(errors(data))

    def test_missing_candidate_metadata_remains_rejected(self):
        for lane in LANES:
            data = event(lane)
            data["pull_request"]["body"] = lane[2]
            with self.subTest(lane=lane[0]):
                self.assertTrue(errors(data))

    def test_head_binding_does_not_complete_mainnet_evidence(self):
        data = event(LANES[1])
        data["pull_request"]["body"] = EVIDENCE
        self.assertTrue(errors(data))

    def test_existing_work_and_reconciliation_routes_are_preserved(self):
        for head, base, body in (
            ("fix/example", "dev", "### Feature -> dev promotion checklist"),
            ("dev", "staged", STAGED),
            ("staged", "main", MAIN),
            ("backmerge/main-to-staged", "staged", ""),
            ("backmerge/staged-to-dev", "dev", ""),
        ):
            data = event(LANES[0])
            data["pull_request"]["head"]["ref"] = head
            data["pull_request"]["base"]["ref"] = base
            data["pull_request"]["body"] = body
            with self.subTest(head=head):
                self.assertEqual([], errors(data))

    def test_process_exit_codes_use_actual_event_head(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "event.json"
            for lane in LANES:
                for head, expected in ((SOURCE, 0), (OTHER, 1), (None, 1)):
                    path.write_text(json.dumps(event(lane, head_sha=head)), encoding="utf-8")
                    result = subprocess.run(
                        [sys.executable, str(SCRIPT), "--event-path", str(path)],
                        cwd=ROOT, capture_output=True, text=True, timeout=10, check=False,
                    )
                    with self.subTest(lane=lane[0], head=head):
                        self.assertEqual(expected, result.returncode, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
