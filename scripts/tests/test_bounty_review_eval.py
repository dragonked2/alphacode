import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from bounty_review_eval import score


class ReviewEvalTests(unittest.TestCase):
    def setUp(self):
        self.run_record = {
            "run_id": "run-1",
            "fixture_revision": "fixture-v2",
            "revision": "agent-v1",
            "model": "model-v1",
            "configuration": "config-v1",
            "budget_seconds": 30,
            "status": "timed_out",
            "expected_root_causes": ["root-1"],
            "reviewed_evidence": [
                {"ref": "current", "fixture_revision": "fixture-v2"},
                {"ref": "old", "fixture_revision": "fixture-v1"},
            ],
            "claims": [],
        }

    def claim(self, claim_id, ref):
        self.run_record["claims"].append({
            "claim_id": claim_id,
            "disposition": "accepted",
            "review_rationale": "Reviewed against the fixture record.",
            "root_cause_id": "root-1",
            "evidence_refs": [ref],
        })

    def test_stale_evidence_gives_no_credit(self):
        self.claim("a", "old")
        report = score({"runs": [self.run_record]})
        self.assertEqual(report["totals"]["stale"], 1)
        self.assertEqual(report["totals"]["missed"], 1)
        self.assertEqual(report["precision"], 0)
        self.assertEqual(report["recall"], 0)

    def test_duplicate_root_and_timeout_remain_visible(self):
        self.claim("a", "current")
        self.claim("b", "current")
        report = score({"runs": [self.run_record]})
        self.assertEqual(report["totals"]["accepted"], 1)
        self.assertEqual(report["totals"]["duplicates"], 1)
        self.assertEqual(report["statuses"], {"timed_out": 1})
        self.assertEqual(report["precision"], 1)
        self.assertEqual(report["unique_accepted_per_claim"], 0.5)

    def test_unknown_evidence_is_rejected(self):
        self.claim("a", "unregistered")
        with self.assertRaisesRegex(ValueError, "unknown reviewed evidence"):
            score({"runs": [self.run_record]})

    def test_family_and_timing_keep_incomplete_runs_visible(self):
        self.run_record["family"] = "authorization"
        self.run_record["seconds_to_reviewed_evidence"] = 4
        self.claim("a", "current")
        incomplete = {
            **self.run_record,
            "run_id": "run-2",
            "family": "configuration",
            "status": "incomplete",
            "claims": [],
            "seconds_to_reviewed_evidence": None,
        }
        report = score({"runs": [self.run_record, incomplete]})
        self.assertEqual(report["by_family"]["authorization"]["recall"], 1)
        self.assertEqual(report["by_family"]["configuration"]["recall"], 0)
        self.assertIsNone(report["by_family"]["configuration"]["precision"])
        self.assertEqual(report["time_to_reviewed_evidence"], {
            "median_observed_seconds": 4,
            "runs_with_timing": 1,
            "runs_without_timing": 1,
        })

    def test_participant_scores_preserve_stale_claims_and_run_status(self):
        self.run_record.update(participant="alphacode", status="completed")
        self.claim("a", "current")
        human = {
            **self.run_record,
            "run_id": "run-2",
            "participant": "human-reviewer",
            "status": "timed_out",
            "claims": [{**self.run_record["claims"][0], "evidence_refs": ["old"]}],
        }
        participants = score({"runs": [self.run_record, human]})["by_participant"]
        self.assertEqual(participants["alphacode"]["precision"], 1)
        self.assertEqual(participants["human-reviewer"]["precision"], 0)
        self.assertEqual(participants["human-reviewer"]["totals"]["stale"], 1)
        self.assertEqual(participants["human-reviewer"]["statuses"], {"timed_out": 1})

    def test_invalid_array_is_not_scored_as_empty(self):
        self.run_record["claims"] = {}
        with self.assertRaisesRegex(ValueError, "claims must be an array"):
            score({"runs": [self.run_record]})
        with self.assertRaisesRegex(ValueError, "runs must be an array"):
            score({"runs": {}})
