"""Safety/consistency rejection tests; actual restore runs against CI PostgreSQL."""
import copy
import json
from pathlib import Path
import tempfile
import unittest

from verify_workflow_restore import fingerprint, validate_container, validate_snapshot, verify


class WorkflowRecoveryBoundaryTests(unittest.TestCase):
    def fixture(self):
        return {"Id": "a" * 64, "State": {"Running": True}, "Config": {
            "Image": "postgres:17", "Labels": {
                "conxian.fixture": "nexus-workflow-recovery", "conxian.fixture.role": "source"}}}

    def snapshot(self):
        schema = "nexus_conformance_" + "a" * 32
        return {f"{schema}.admin_workflow_entities": [{"id": "artifact-1", "status": "approved"}],
                f"{schema}.admin_workflow_events": [{"id": "audit-1", "receipt_json": json.dumps({
                    "accepted": True, "auditEventId": "audit-1"})}]}

    def test_requires_exact_labelled_running_fixture(self):
        validate_container(self.fixture(), "a" * 64, "source")
        for change in ["wrong_id", "unlabelled", "wrong_image", "wrong_role", "stopped"]:
            info = copy.deepcopy(self.fixture())
            if change == "wrong_id":
                info["Id"] = "b" * 64
            elif change == "unlabelled":
                info["Config"]["Labels"] = {}
            elif change == "wrong_image":
                info["Config"]["Image"] = "postgres:latest"
            elif change == "wrong_role":
                info["Config"]["Labels"]["conxian.fixture.role"] = "restore"
            else:
                info["State"]["Running"] = False
            with self.subTest(change=change), self.assertRaises(ValueError):
                validate_container(info, "a" * 64, "source")

    def test_same_or_partial_container_ids_fail_before_docker(self):
        with tempfile.TemporaryDirectory() as temp:
            for source, target in [("a" * 64, "a" * 64), ("a", "b" * 64), ("prod", "staging")]:
                with self.subTest(source=source), self.assertRaises(ValueError):
                    verify(source, target, "c" * 40, Path(temp) / "receipt.json")

    def test_rejects_empty_or_partial_workflow_snapshot(self):
        for snapshot in [{}, {next(iter(self.snapshot())): []}, {"public.admin_workflow_entities": []}]:
            with self.subTest(snapshot=snapshot), self.assertRaises(ValueError):
                validate_snapshot(snapshot)

    def test_audit_receipt_must_bind_its_event(self):
        snapshot = self.snapshot()
        validate_snapshot(snapshot)
        event = list(snapshot.values())[1][0]
        for receipt in [{"accepted": False, "auditEventId": "audit-1"},
                        {"accepted": True, "auditEventId": "different"}, []]:
            event["receipt_json"] = json.dumps(receipt)
            with self.subTest(receipt=receipt), self.assertRaises(ValueError):
                validate_snapshot(snapshot)

    def test_snapshot_digest_detects_receipt_state_or_notes_changes(self):
        before = self.snapshot()
        for field in ["status", "notes", "receipt_json"]:
            after = copy.deepcopy(before)
            list(after.values())[0][0][field] = "changed"
            with self.subTest(field=field):
                self.assertNotEqual(fingerprint(before), fingerprint(after))
        self.assertEqual(fingerprint(before), fingerprint(dict(reversed(list(before.items())))))


if __name__ == "__main__":
    unittest.main()
