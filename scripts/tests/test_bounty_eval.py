import json
from pathlib import Path
import sys
import tempfile
import unittest
from urllib.error import HTTPError
from urllib.request import Request, urlopen

sys.path.insert(0, str(Path(__file__).parents[1]))
import bounty_eval


def get(state, actor, path):
    request = Request(state["base_url"] + path, headers={"Authorization": "Bearer " + state["tokens"][actor]})
    try:
        with urlopen(request, timeout=3) as response:
            return response.status, json.load(response)
    except HTTPError as error:
        with error:
            return error.code, json.load(error)


class BountyEvalTests(unittest.TestCase):
    def test_leak_requires_actual_repeated_cross_actor_access_and_controls(self):
        with bounty_eval.target("vulnerable") as state:
            path = "/api/exports/" + state["ids"]["b"]
            finding = {"actor": "a", "url": state["base_url"] + path, "marker": state["markers"]["b"]}
            self.assertEqual(bounty_eval.grade(state, [finding])["false_positives"], 1)
            self.assertEqual(get(state, "b", path)[0], 200)
            self.assertEqual(get(state, "a", path)[1]["content"], finding["marker"])
            get(state, "a", "/api/exports/nonexistent")
            self.assertEqual(bounty_eval.grade(state, [finding])["true_positives"], 0)
            get(state, "a", path)
            metrics = bounty_eval.grade(state, [finding, finding])
            self.assertEqual(metrics["true_positives"], 1)
            self.assertEqual(metrics["duplicates"], 1)
            self.assertEqual(metrics["false_negatives"], 0)

    def test_repaired_and_public_variants_reject_false_claims(self):
        for variant in ("repaired", "public"):
            with self.subTest(variant=variant), bounty_eval.target(variant) as state:
                path = "/api/exports/" + state["ids"]["b"]
                self.assertEqual(get(state, "a", path)[0], 200 if variant == "public" else 403)
                get(state, "a", path)
                get(state, "b", path)
                get(state, "a", "/api/exports/nonexistent")
                result = bounty_eval.grade(state, [{"actor": "a", "url": state["base_url"] + path, "marker": state["markers"]["b"]}])
                self.assertEqual(result["false_positives"], 1)
                self.assertEqual(result["expected_root_causes"], 0)

    def test_budget_stops_access_and_does_not_log_bearer_secrets(self):
        with bounty_eval.target("vulnerable", 1) as state:
            self.assertEqual(get(state, "a", "/api/documents")[0], 200)
            self.assertEqual(get(state, "a", "/api/documents")[0], 429)
            self.assertNotIn(state["tokens"]["a"], json.dumps(state["requests"]))
            self.assertEqual(state["request_count"], 2)

    def test_successful_agent_exit_is_not_discovery_and_missing_report_is_recorded(self):
        with tempfile.TemporaryDirectory() as directory:
            report = bounty_eval.evaluate([sys.executable, "-c", "pass"], Path(directory) / "report.json", "no-op fixture agent", timeout=5)
            self.assertEqual(report["recall"], 0)
            self.assertIsNone(report["precision"])
            self.assertEqual(len(report["trials"]), 3)
            self.assertTrue(all(t["report_error"] for t in report["trials"]))
            self.assertTrue(all(t["agent"]["exit_code"] == 0 for t in report["trials"]))
            self.assertEqual(report["completed_assessments"], 0)
            self.assertEqual(report["incomplete_assessments"], 3)

    def test_runner_credits_network_evidence_and_rejects_public_access(self):
        # A deterministic fixture client tests the runner plumbing. This is
        # deliberately not represented as an LLM discovery measurement.
        client = '''
import json,re
from pathlib import Path
from urllib.request import Request,urlopen
from urllib.error import HTTPError
task=Path('TASK.md').read_text()
base=re.search(r'http://127[.]0[.]0[.]1:[0-9]+',task)[0]
tokens={a:re.search(a+r'=([a-f0-9]+)',task)[1] for a in ('a','b')}
def get(actor,path):
    try:
        with urlopen(Request(base+path,headers={'Authorization':'Bearer '+tokens[actor]}),timeout=3) as response:
            return response.status,json.load(response)
    except HTTPError as error:
        with error:
            return error.code,json.load(error)
policy=get('a','/')[1]['policy']
item=get('b','/api/documents')[1]['documents'][0]['id']
path='/api/exports/'+item
owner=get('b',path)
first=get('a',path)
second=get('a',path)
missing=get('a','/api/exports/nonexistent')
findings=[]
if 'private' in policy and first[0]==second[0]==200 and owner[1]['content']==first[1]['content'] and missing[0]==404:
    findings=[{'url':base+path,'actor':'a','marker':first[1]['content']}]
Path('findings.json').write_text(json.dumps(findings))
'''
        with tempfile.TemporaryDirectory() as directory:
            report = bounty_eval.evaluate([sys.executable, "-c", client], Path(directory) / "report.json", "scripted harness self-test", timeout=10)
            self.assertEqual(report["totals"]["true_positives"], 1)
            self.assertEqual(report["totals"]["false_positives"], 0)
            self.assertEqual(report["totals"]["false_negatives"], 0)
            self.assertEqual(report["precision"], 1)
            self.assertTrue(all(not t["report_error"] for t in report["trials"]))
            self.assertEqual(report["completed_assessments"], 3)
            self.assertEqual(report["incomplete_assessments"], 0)


if __name__ == "__main__":
    unittest.main()
