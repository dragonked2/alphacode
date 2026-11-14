"""Offline tests of grounding, truthful coverage and teacher failure handling."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("report_study", Path(__file__).parents[1] / "report_study.py")
sys.path.insert(0, str(Path(__file__).parents[1]))
study = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(study)


class ReportStudyTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.body = "The removed member still retrieved the private fixture. The owner verified removal."
        study.write_json(self.root / "sources/101.json", {"id": 101, "public": True, "vulnerability_information": self.body})
        study.write_json(self.root / "progress.json", {"version": 1, "reports": {
            "101": {"collection": "narrative", "study": "pending"},
            "102": {"collection": "pending", "study": "pending"}}})
        template = Path(__file__).parents[2] / "src/alphacode_security_core/reasoning_lessons.json"
        self.curriculum = json.loads(template.read_text(encoding="utf-8"))
        self.curriculum["lessons"] = [self.curriculum["lessons"][0]]
        self.curriculum["lessons"][0]["sources"] = [{"report_id": 101,
            "basis": "narrative", "content_sha256": study.digest(self.body.encode()),
            "observation": "A removed member retained private read capability.",
            "evidence_quote": "removed member still retrieved the private fixture"}]

    def test_metadata_and_private_payloads_cannot_become_narrative_learning(self):
        for payload in [[], None, {"id": 101, "public": True, "title": "Critical IDOR"},
                        {"id": 101, "public": False, "vulnerability_information": self.body},
                        {"id": 999, "public": True, "vulnerability_information": self.body}]:
            with self.assertRaises(ValueError):
                study.narrative(payload, 101)

    def test_summary_basis_is_preserved_and_cannot_be_upgraded(self):
        study.write_json(self.root / "sources/101.json", {"id": 101, "public": True,
            "vulnerability_information": "", "summaries": [{"content": self.body}]})
        with self.assertRaises(ValueError):
            study.validate(self.curriculum, self.root)
        self.curriculum["lessons"][0]["sources"][0]["basis"] = "summary"
        self.assertEqual(study.validate(self.curriculum, self.root), {101})

    def test_invented_anchor_stale_digest_and_missing_counterexample_fail(self):
        for field, value in [("evidence_quote", "nonexistent quote"), ("content_sha256", "0" * 64)]:
            draft = copy.deepcopy(self.curriculum)
            draft["lessons"][0]["sources"][0][field] = value
            with self.assertRaises(ValueError):
                study.validate(draft, self.root)
        self.curriculum["lessons"][0]["counterexamples"] = []
        with self.assertRaises(ValueError):
            study.validate(self.curriculum, self.root)

    def test_compile_counts_only_grounded_reports_and_retains_pending(self):
        draft, output = self.root / "draft.json", self.root / "compiled.json"
        study.write_json(draft, self.curriculum)
        progress = study.compile_curriculum(self.root, draft, output)
        self.assertEqual(progress["reports"]["101"]["study"], "grounded_lesson")
        self.assertEqual(progress["reports"]["102"]["study"], "pending")
        self.assertEqual(study.summary(progress)["catalog_reports"], 2)
        self.assertEqual(progress["compiled_sha256"], study.digest(output.read_bytes()))

    def test_compiling_reviewed_subset_preserves_other_unreviewed_drafts(self):
        progress = json.loads((self.root / "progress.json").read_bytes())
        progress["reports"]["102"]["study"] = "drafted_needs_review"
        study.write_json(self.root / "progress.json", progress)
        draft = self.root / "reviewed.json"
        study.write_json(draft, self.curriculum)
        result = study.compile_curriculum(self.root, draft, self.root / "compiled.json")
        self.assertEqual(result["reports"]["102"]["study"], "drafted_needs_review")

    def test_a_backend_failure_stops_the_batch_before_more_model_calls(self):
        progress = json.loads((self.root / "progress.json").read_bytes())
        progress["reports"]["102"]["collection"] = "narrative"
        study.write_json(self.root / "progress.json", progress)
        with mock.patch.object(study.agent_eval, "run_command", return_value={"status":"completed","exit_code":1}) as teacher:
            result = study.draft_lessons(self.root, ["teacher","{input}","{output}"],2,5)
        self.assertEqual(teacher.call_count,1)
        self.assertEqual(result["reports"]["102"]["study"],"pending")

    def test_failed_compile_preserves_previous_curriculum(self):
        draft, output = self.root / "bad.json", self.root / "compiled.json"
        output.write_bytes(b"previous valid artifact")
        self.curriculum["lessons"][0]["sources"][0]["basis"] = "title"
        study.write_json(draft, self.curriculum)
        with self.assertRaises(ValueError):
            study.compile_curriculum(self.root, draft, output)
        self.assertEqual(output.read_bytes(), b"previous valid artifact")

    def test_rate_limit_checkpoints_and_stops_the_whole_pass(self):
        progress = json.loads((self.root / "progress.json").read_bytes())
        progress["reports"]["101"]["collection"] = "pending"
        (self.root / "sources/101.json").unlink()
        study.write_json(self.root / "progress.json", progress)
        error = study.urllib.error.HTTPError("https://hackerone.com/reports/101.json", 429, "rate limited", {}, None)
        with mock.patch.object(study, "download", side_effect=error) as fetch:
            result = study.collect(self.root, 2, 1, set())
        self.assertEqual(fetch.call_count, 1)
        self.assertEqual(result["reports"]["101"]["collection"], "rate_limited")
        self.assertEqual(result["reports"]["102"]["collection"], "pending")

    def test_teacher_success_without_fresh_output_is_not_learning(self):
        command = [sys.executable, "-c", "pass", "{input}", "{output}"]
        result = study.draft_lessons(self.root, command, 1, 5)
        self.assertEqual(result["reports"]["101"]["study"], "draft_failed")

    def test_teacher_grounded_output_is_still_a_draft_not_a_reviewed_lesson(self):
        payload = json.dumps(self.curriculum)
        command = [sys.executable, "-c", "import pathlib,sys;pathlib.Path(sys.argv[2]).write_text(sys.argv[3],encoding='utf8')", "{input}", "{output}", payload]
        result = study.draft_lessons(self.root, command, 1, 5)
        self.assertEqual(result["reports"]["101"]["study"], "drafted_needs_review")
        self.assertNotIn("grounded_lesson", study.summary(result)["study"])

    def test_catalog_is_revision_pinned(self):
        with self.assertRaises(ValueError):
            study.catalog(b'{"reports": []}')

    def test_a_collector_and_teacher_cannot_race_the_same_coverage_file(self):
        with study.study_writer(self.root):
            with self.assertRaises(ValueError):
                with study.study_writer(self.root):
                    self.fail("second writer entered")
        self.assertFalse((self.root / "writer.lock").exists())


if __name__ == "__main__":
    unittest.main()
