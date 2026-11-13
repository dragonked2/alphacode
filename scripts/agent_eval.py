#!/usr/bin/env python3
"""Run coding agents against fresh fixtures and independent Python verifiers.

Commands are argument arrays, never shell strings. This isolates task files,
not the agent process: run only agents and verifiers you trust.
"""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import runpy
import secrets
import shutil
import signal
import subprocess
import sys
import tempfile
import time


def bounded_path(root, value):
    path = (root / value).resolve()
    if not path.is_relative_to(root):
        raise ValueError(f"Task path escapes the manifest directory: {value}")
    return path


def load_tasks(manifest):
    root = manifest.resolve().parent
    tasks = json.loads(manifest.read_text(encoding="utf-8"))["tasks"]
    if not tasks:
        raise ValueError("The manifest must contain at least one task")
    seen = set()
    for task in tasks:
        name = task["id"]
        if not isinstance(name, str) or not re.fullmatch(r"[A-Za-z0-9_-]+", name):
            raise ValueError("Task IDs must contain only letters, digits, _ or -")
        if name in seen:
            raise ValueError(f"Duplicate task ID: {name}")
        seen.add(name)
        if not isinstance(task["prompt"], str) or not task["prompt"].strip():
            raise ValueError(f"Task {name} needs a prompt")
        fixture = bounded_path(root, task["fixture"])
        verifier = bounded_path(root, task["verifier"])
        if not fixture.is_dir() or not verifier.is_file():
            raise ValueError(f"Missing fixture or verifier for {name}")
        files = sorted(fixture.rglob("*"))
        if any(path.is_symlink() for path in files):
            raise ValueError(f"Fixture {name} contains a symbolic link")
        digest = hashlib.sha256(verifier.read_bytes())
        for path in files:
            if path.is_file():
                digest.update(path.relative_to(fixture).as_posix().encode() + b"\0")
                digest.update(hashlib.sha256(path.read_bytes()).digest())
        task["fixture_path"] = fixture
        task["verifier_path"] = verifier
        task["fingerprint"] = digest.hexdigest()
    return tasks


def stop_process_tree(process):
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/PID", str(process.pid), "/T", "/F"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=subprocess.CREATE_NO_WINDOW,
            check=False,
        )
    else:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    if process.poll() is None:
        process.kill()
    process.wait()


def run_command(argv, cwd, timeout, log):
    started = time.monotonic()
    options = (
        {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP | subprocess.CREATE_NO_WINDOW}
        if os.name == "nt"
        else {"start_new_session": True}
    )
    with log.open("wb") as output:
        try:
            process = subprocess.Popen(
                argv, cwd=cwd, stdout=output, stderr=subprocess.STDOUT,
                stdin=subprocess.DEVNULL, **options,
            )
        except OSError as error:
            output.write(str(error).encode("utf-8", errors="replace"))
            return {"status": "launch_error", "exit_code": None, "seconds": time.monotonic() - started}
        try:
            exit_code = process.wait(timeout=timeout)
            status = "completed"
        except subprocess.TimeoutExpired:
            stop_process_tree(process)
            exit_code, status = process.returncode, "timed_out"
        except BaseException:
            stop_process_tree(process)
            raise
    return {"status": status, "exit_code": exit_code, "seconds": time.monotonic() - started}


def write_report(path, report):
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=path.parent, delete=False) as scratch:
        scratch_path = Path(scratch.name)
        try:
            json.dump(report, scratch, indent=2, ensure_ascii=False)
            scratch.write("\n")
        except BaseException:
            scratch.close()
            scratch_path.unlink(missing_ok=True)
            raise
    try:
        scratch_path.replace(path)
    finally:
        scratch_path.unlink(missing_ok=True)


def verify_worker(verifier, workspace, receipt, token):
    sys.argv = [verifier, workspace]
    runpy.run_path(verifier, run_name="__main__")
    # A task module calling exit(0) must not turn an unfinished verifier into
    # a passing result. Only normal completion reaches this exclusive write.
    with Path(receipt).open("x", encoding="ascii") as output:
        output.write(token)


def evaluate(manifest, command, label, output, timeout=300, verify_timeout=60, repetitions=1):
    if not isinstance(command, list) or not command or not all(isinstance(arg, str) for arg in command) or not command[0]:
        raise ValueError("Agent command must be a nonempty JSON array of strings")
    if not all(math.isfinite(value) and value > 0 for value in (timeout, verify_timeout)) or repetitions < 1:
        raise ValueError("Timeouts and repetitions must be positive")
    tasks = load_tasks(manifest)
    output = output.resolve()
    if output.exists():
        raise FileExistsError(f"Refusing to overwrite an existing report: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    artifacts = Path(tempfile.mkdtemp(prefix=output.stem + "-", dir=output.parent))
    report = {
        "schema_version": 1, "label": label, "status": "running",
        "manifest_sha256": hashlib.sha256(manifest.read_bytes()).hexdigest(),
        "artifacts": artifacts.name, "planned_trials": len(tasks) * repetitions,
        "agent_timeout_seconds": timeout, "verifier_timeout_seconds": verify_timeout,
        "trials": [],
    }
    write_report(output, report)
    for repetition in range(1, repetitions + 1):
        for task in tasks:
            trial_dir = artifacts / f"{task['id']}-{repetition}"
            workspace = trial_dir / "workspace"
            shutil.copytree(task["fixture_path"], workspace)
            replacements = {"prompt": task["prompt"], "workspace": str(workspace), "python": sys.executable}
            argv = [re.sub(r"\{(prompt|workspace|python)\}", lambda match: replacements[match[1]], arg) for arg in command]
            agent = run_command(argv, workspace, timeout, trial_dir / "agent.log")
            # Keep the verifier outside the copied workspace, so editing the
            # task's visible tests does not replace the evaluator's checks.
            token = secrets.token_hex(32)
            receipt = trial_dir / ("receipt-" + token[:16] + ".txt")
            verifier = run_command(
                [sys.executable, str(Path(__file__).resolve()), "--verify-worker",
                 str(task["verifier_path"]), str(workspace), str(receipt), token],
                workspace, verify_timeout, trial_dir / "verifier.log",
            )
            try:
                verifier["receipt_valid"] = not receipt.is_symlink() and receipt.read_text(encoding="ascii") == token
            except (OSError, UnicodeError):
                verifier["receipt_valid"] = False
            passed = verifier["receipt_valid"] and all(result["status"] == "completed" and result["exit_code"] == 0 for result in (agent, verifier))
            report["trials"].append({
                "task": task["id"], "repetition": repetition,
                "task_sha256": task["fingerprint"], "passed": passed,
                "agent": agent, "verifier": verifier,
                "directory": trial_dir.relative_to(artifacts).as_posix(),
            })
            write_report(output, report)
    report["status"] = "complete"
    report["passed"] = sum(trial["passed"] for trial in report["trials"])
    report["success_rate"] = report["passed"] / report["planned_trials"]
    write_report(output, report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--agent-command", type=json.loads, required=True, help='JSON argv array; placeholders: {prompt}, {workspace}, {python}')
    parser.add_argument("--label", required=True, help="Agent, version, and model description")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=300)
    parser.add_argument("--verify-timeout", type=float, default=60)
    parser.add_argument("--repetitions", type=int, default=1)
    args = parser.parse_args()
    try:
        report = evaluate(args.manifest, args.agent_command, args.label, args.output,
                          args.timeout, args.verify_timeout, args.repetitions)
    except (ValueError, OSError, KeyError, TypeError) as error:
        parser.error(str(error))
    print(json.dumps({"report": str(args.output.resolve()), "passed": report["passed"],
                      "trials": report["planned_trials"], "success_rate": report["success_rate"]}))
    return 0 if report["passed"] == report["planned_trials"] else 1


if __name__ == "__main__":
    if len(sys.argv) == 6 and sys.argv[1] == "--verify-worker":
        verify_worker(*sys.argv[2:])
    else:
        raise SystemExit(main())
