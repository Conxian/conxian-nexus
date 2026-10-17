"""Keep the coverage gate attached to its intended code as source lines move."""
from pathlib import Path
import tempfile
import unittest

from check_lightning_coverage import resolve_scoped_ranges


class CoverageSourceContracts(unittest.TestCase):
    def resolve(self, source):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "sample.rs").write_text(source)
            return resolve_scoped_ranges(root, {"sample.rs": [("fn selected()", "\nfn next()")]})

    def test_unrelated_insertions_move_the_measured_block(self):
        original = "fn selected() {\n    important();\n}\nfn next() {}\n"
        self.assertEqual(self.resolve(original), {"sample.rs": [(1, 3)]})
        self.assertEqual(self.resolve("// inserted\n\n" + original), {"sample.rs": [(3, 5)]})

    def test_missing_or_duplicate_anchors_fail_closed(self):
        for source in ["fn next() {}\n", "fn selected() {}\n",
                       "fn selected() {}\nfn selected() {}\nfn next() {}\n",
                       "fn selected() {}\nfn next() {}\nfn next() {}\n"]:
            with self.subTest(source=source), self.assertRaisesRegex(RuntimeError, "anchors"):
                self.resolve(source)

    def test_reordered_anchors_fail_closed(self):
        with self.assertRaisesRegex(RuntimeError, "Reordered"):
            self.resolve("// source\nfn next() {}\nfn selected() {}\n")

    def test_repository_scopes_resolve_without_overlapping(self):
        root = Path(__file__).resolve().parents[1]
        for ranges in resolve_scoped_ranges(root).values():
            for index, (start, end) in enumerate(ranges):
                self.assertLessEqual(start, end)
                if index:
                    self.assertGreater(start, ranges[index - 1][1])


if __name__ == "__main__":
    unittest.main()
