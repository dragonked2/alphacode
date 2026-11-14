import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).parents[1]))
import prepare_known_fix_review as review

CASE = Path(__file__).resolve().parents[2] / "tests/known_fix_review/bitcoin-cve-2024-35202.json"


class KnownFixReviewTests(unittest.TestCase):
    def test_preparation_pins_sources_and_records_exact_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "fixture"
            requested = []
            def fetch(url):
                requested.append(url)
                return url.encode()
            result = review.prepare(CASE, output, fetch)
            self.assertEqual(len(requested), 8)
            for item in result["files"]:
                data = (output / item["snapshot"] / item["path"]).read_bytes()
                self.assertEqual(hashlib.sha256(data).hexdigest(), item["sha256"])
                self.assertIn(item["revision"], item["url"])
            self.assertIn("not a blind", (output / "TASK.md").read_text())
            self.assertFalse((output / "review.md").exists())
            with self.assertRaises(FileExistsError):
                review.prepare(CASE, output, fetch)
            self.assertEqual(len(requested), 8)

    def test_rejects_unpinned_or_executable_inputs(self):
        original = json.loads(CASE.read_text())
        for field, value in [("fixed_revision", "master"), ("paths", ["../outside"]),
                             ("paths", ["src/test.py"]), ("paths", ["C:/outside.h"]),
                             ("paths", ["src/a.h", "src/a.h"])]:
            with self.subTest(field=field, value=value):
                with self.assertRaises(ValueError):
                    review.validate(dict(original, **{field: value}))

    def test_failed_download_does_not_publish_a_partial_fixture(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "fixture"
            def fail(url):
                raise OSError("fixture download failed")
            with self.assertRaises(OSError):
                review.prepare(CASE, output, fail)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
