import importlib.util
import json
from pathlib import Path
import sys
import subprocess
import tempfile
import time
import unittest


spec = importlib.util.spec_from_file_location("agent_eval", Path(__file__).parents[1] / "agent_eval.py")
agent_eval = importlib.util.module_from_spec(spec)
spec.loader.exec_module(agent_eval)


class AgentEvalTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="alphacode-eval-tests-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        fixture = self.root / "fixture"
        fixture.mkdir()
        (fixture / "answer.txt").write_text("broken", encoding="utf-8")
        (self.root / "verify.py").write_text(
            "import pathlib,sys\nassert (pathlib.Path(sys.argv[1])/'answer.txt').read_text() == 'fixed'\n",
            encoding="utf-8",
        )
        self.manifest = self.root / "tasks.json"
        self.manifest.write_text(json.dumps({"tasks": [{
            "id": "repair", "prompt": "Fix the answer", "fixture": "fixture", "verifier": "verify.py",
        }]}), encoding="utf-8")

    def run_agent(self, code, **kwargs):
        return agent_eval.evaluate(
            self.manifest, [sys.executable, "-c", code], "fixture agent", self.root / "report.json", **kwargs,
        )

    def test_fresh_trials_verify_repairs_without_mutating_original_fixture(self):
        report = self.run_agent(
            "from pathlib import Path; p=Path('answer.txt'); assert p.read_text()=='broken'; p.write_text('fixed')",
            repetitions=2,
        )
        self.assertEqual(report["status"], "complete")
        self.assertEqual(report["passed"], 2)
        self.assertEqual(report["success_rate"], 1)
        self.assertEqual((self.root / "fixture/answer.txt").read_text(), "broken")
        self.assertEqual(len({trial["directory"] for trial in report["trials"]}), 2)
        self.assertEqual(json.loads((self.root / "report.json").read_text()), report)

    def test_successful_agent_exit_does_not_replace_verifier_evidence(self):
        report = self.run_agent("pass")
        self.assertEqual(report["trials"][0]["agent"]["exit_code"], 0)
        self.assertNotEqual(report["trials"][0]["verifier"]["exit_code"], 0)
        self.assertEqual(report["passed"], 0)

    def test_arguments_remain_literal_and_prompt_substitution_is_not_recursive(self):
        prompt = "Fix the answer; $(do_not_execute) {workspace}"
        manifest = json.loads(self.manifest.read_text())
        manifest["tasks"][0]["prompt"] = prompt
        self.manifest.write_text(json.dumps(manifest))
        code = (
            f"import sys; from pathlib import Path; assert sys.argv[1]==''; "
            f"assert sys.argv[2].encode().hex()=={prompt.encode().hex()!r}; Path('answer.txt').write_text('fixed')"
        )
        report = agent_eval.evaluate(
            self.manifest, ["{python}", "-c", code, "", "{prompt}"],
            "literal arguments", self.root / "report.json",
        )
        self.assertEqual(report["passed"], 1)

    def test_failed_agent_is_recorded_even_when_verifier_passes(self):
        report = self.run_agent("from pathlib import Path; Path('answer.txt').write_text('fixed'); raise SystemExit(7)")
        self.assertEqual(report["trials"][0]["agent"]["exit_code"], 7)
        self.assertEqual(report["trials"][0]["verifier"]["exit_code"], 0)
        self.assertEqual(report["passed"], 0)

    def test_timed_out_agent_is_stopped_and_reported_as_failure(self):
        report = self.run_agent("import time; time.sleep(30)", timeout=0.2)
        trial = report["trials"][0]
        self.assertEqual(trial["agent"]["status"], "timed_out")
        self.assertLess(trial["agent"]["seconds"], 10)
        self.assertFalse(trial["passed"])

    def test_missing_executable_is_reported_without_claiming_completion(self):
        report = agent_eval.evaluate(self.manifest, [str(self.root / "missing.exe")], "missing", self.root / "report.json")
        self.assertEqual(report["trials"][0]["agent"]["status"], "launch_error")
        self.assertEqual(report["passed"], 0)

    def test_verifier_timeout_is_not_a_successful_task(self):
        (self.root / "verify.py").write_text("import time\ntime.sleep(30)\n")
        report = self.run_agent("from pathlib import Path; Path('answer.txt').write_text('fixed')", verify_timeout=0.2)
        trial = report["trials"][0]
        self.assertEqual(trial["agent"]["exit_code"], 0)
        self.assertEqual(trial["verifier"]["status"], "timed_out")
        self.assertFalse(trial["verifier"]["receipt_valid"])
        self.assertFalse(trial["passed"])

    def test_cli_returns_failure_and_keeps_machine_readable_evidence(self):
        output = self.root / "cli-report.json"
        result = subprocess.run(
            [sys.executable, str(Path(agent_eval.__file__)), "--manifest", str(self.manifest),
             "--agent-command", json.dumps([sys.executable, "-c", "pass"]),
             "--label", "CLI fixture", "--output", str(output)],
            capture_output=True, text=True, timeout=10, check=False,
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(json.loads(result.stdout)["passed"], 0)
        self.assertEqual(json.loads(output.read_text())["status"], "complete")

    def test_timeout_stops_descendants(self):
        ready = self.root / "child-started"
        escaped = self.root / "child-survived"
        child = f"import time; from pathlib import Path; time.sleep(3); Path({str(escaped)!r}).write_text('survived')"
        parent = (
            f"import subprocess,sys,time; from pathlib import Path; "
            f"subprocess.Popen([sys.executable,'-c',{child!r}]); "
            f"Path({str(ready)!r}).write_text('ready'); time.sleep(30)"
        )
        report = self.run_agent(parent, timeout=1.5)
        self.assertTrue(ready.exists(), "fixture must actually start its child")
        self.assertEqual(report["trials"][0]["agent"]["status"], "timed_out")
        time.sleep(3.1)
        self.assertFalse(escaped.exists(), "timed-out agent must not leave its child running")

    def test_bundled_verifiers_reject_broken_fixtures_and_accept_known_repairs(self):
        manifest = Path(__file__).parents[2] / "tests/agent_eval/tasks.json"
        baseline = agent_eval.evaluate(manifest, [sys.executable, "-c", "pass"], "broken fixtures", self.root / "baseline.json")
        self.assertEqual(baseline["passed"], 0)
        self.assertEqual(baseline["planned_trials"], 2)
        quantity = (
            "import math\n"
            "def parse_quantity(value):\n"
            "    if isinstance(value, bool) or not isinstance(value, (str, int, float)):\n"
            "        raise ValueError('unsupported quantity')\n"
            "    try:\n"
            "        number = float(value)\n"
            "    except (ValueError, OverflowError) as error:\n"
            "        raise ValueError('invalid quantity') from error\n"
            "    if not math.isfinite(number) or number < 0:\n"
            "        raise ValueError('invalid quantity')\n"
            "    return number\n"
        )
        ordered = "def unique_in_order(values):\n    return list(dict.fromkeys(values))\n"
        repair = (
            "from pathlib import Path; "
            f"p=Path('quantity.py'); p.write_text({quantity!r}) if p.exists() else Path('ordered.py').write_text({ordered!r})"
        )
        fixed = agent_eval.evaluate(manifest, [sys.executable, "-c", repair], "known fixture repairs", self.root / "fixed.json")
        self.assertEqual(fixed["passed"], 2)

    def test_existing_report_is_preserved(self):
        output = self.root / "report.json"
        output.write_text("previous evidence")
        with self.assertRaises(FileExistsError):
            self.run_agent("pass")
        self.assertEqual(output.read_text(), "previous evidence")

    def test_early_zero_exit_from_task_code_cannot_pass_the_verifier(self):
        manifest = Path(__file__).parents[2] / "tests/agent_eval/tasks.json"
        exit_early = "from pathlib import Path; [p.write_text('raise SystemExit(0)\\n') for p in Path('.').glob('*.py')]"
        report = agent_eval.evaluate(
            manifest, [sys.executable, "-c", exit_early], "early exit fixture",
            self.root / "early-exit.json",
        )
        self.assertEqual(report["passed"], 0)
        for trial in report["trials"]:
            self.assertEqual(trial["verifier"]["exit_code"], 0)
            self.assertFalse(trial["verifier"]["receipt_valid"])

    def test_non_array_command_and_invalid_budgets_are_rejected(self):
        for command, timeout in [("python", 1), ([], 1), (["python"], float("nan")), (["python"], 0)]:
            with self.subTest(command=command, timeout=timeout), self.assertRaises(ValueError):
                agent_eval.evaluate(self.manifest, command, "invalid", self.root / "report.json", timeout=timeout)

    def test_manifest_cannot_copy_paths_outside_its_directory(self):
        task = json.loads(self.manifest.read_text())
        task["tasks"][0]["fixture"] = "../outside"
        self.manifest.write_text(json.dumps(task))
        with self.assertRaisesRegex(ValueError, "escapes"):
            agent_eval.load_tasks(self.manifest)


if __name__ == "__main__":
    unittest.main()
