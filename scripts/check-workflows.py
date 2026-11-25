#!/usr/bin/env python3
"""Validate .github/workflows/*.yml without needing actionlint or a runner.

Exists because workflow defects are otherwise discovered by pushing a tag and
waiting for a release build to fail. The checks here cover the mistakes that
have actually happened in this repository or that GitHub rejects at load time:

* YAML that does not parse at all.
* `uses: owner/repo@ref` whose ref does not exist upstream (a typo'd tag makes
  the whole job fail at startup).
* `secrets.*` used in a step-level `if:` -- the `secrets` context is not
  available there, and GitHub rejects the *file* at load time.
* Step `uses:` actions that do not exist.
* A `steps.<id>` referenced before the step that defines it, which silently
  evaluates to an empty string.
* Shell bodies that bash cannot parse (the CI runner would die mid-step).

Usage:
    python scripts/check-workflows.py            # offline checks
    python scripts/check-workflows.py --remote   # also verify action refs exist

Exit codes:
    0  clean
    1  problems found
    2  usage / environment error
"""

from __future__ import annotations

import glob
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request

WORKFLOW_GLOB = os.path.join(".github", "workflows", "*.yml")
USES_RE = re.compile(r"^\s*(?:-\s*)?uses:\s*(\S+)")
STEP_ID_RE = re.compile(r"^\s*(?:-\s*)?id:\s*(\S+)")
STEP_REF_RE = re.compile(r"steps\.([A-Za-z0-9_-]+)\.")
SECRET_IN_IF_RE = re.compile(r"if:.*secrets\.")


def load_yaml():
    try:
        import yaml  # noqa: F401
    except ImportError:
        return None
    return yaml


def check_yaml(yaml, path, problems):
    try:
        with open(path, encoding="utf-8") as handle:
            return yaml.safe_load(handle)
    except Exception as error:  # noqa: BLE001 - report any parse error verbatim
        problems.append(f"{path}: does not parse as YAML: {error}")
        return None


def workflow_steps(document):
    """Yield (job_name, [steps]) for every job that has a step list."""
    jobs = (document or {}).get("jobs") or {}
    if not isinstance(jobs, dict):
        return
    for job_name, job in jobs.items():
        if isinstance(job, dict) and isinstance(job.get("steps"), list):
            yield job_name, job["steps"]


def check_secret_in_if(path, job_name, steps, problems):
    for index, step in enumerate(steps):
        if not isinstance(step, dict):
            continue
        condition = step.get("if")
        if isinstance(condition, str) and "secrets." in condition:
            name = step.get("name") or step.get("uses") or f"steps[{index}]"
            problems.append(
                f"{path}:{job_name}:{name}: `secrets.` is not available in a "
                "step-level `if:`; GitHub rejects the workflow at load time. "
                "Detect it inside a `run:` step and gate later steps on its output."
            )


def check_step_references(path, job_name, steps, problems):
    defined = set()
    for index, step in enumerate(steps):
        if not isinstance(step, dict):
            continue
        # A step can reference an earlier step's output (steps.<id>.outputs...).
        text = json.dumps(step)
        for identifier in STEP_REF_RE.findall(text):
            if identifier not in defined:
                name = step.get("name") or step.get("uses") or f"steps[{index}]"
                problems.append(
                    f"{path}:{job_name}:{name}: references steps.{identifier} "
                    "before any step with that id; the value is silently empty."
                )
        step_id = step.get("id")
        if isinstance(step_id, str):
            defined.add(step_id)


def check_unique_ids(path, job_name, steps, problems):
    seen = {}
    for index, step in enumerate(steps):
        if isinstance(step, dict) and isinstance(step.get("id"), str):
            step_id = step["id"]
            if step_id in seen:
                problems.append(
                    f"{path}:{job_name}: duplicate step id '{step_id}' "
                    f"(steps {seen[step_id]} and {index})."
                )
            seen[step_id] = index


def check_shell_bodies(path, job_name, steps, problems):
    """Run `bash -n` on each multi-line run body.

    GitHub executes `run:` under bash (or the declared shell); a syntax error
    there fails the step after the job has already spent its setup time. An
    unbalanced quote or a stray heredoc shows up here in milliseconds instead.
    """
    for index, step in enumerate(steps):
        if not isinstance(step, dict):
            continue
        body = step.get("run")
        if not isinstance(body, str) or "\n" not in body:
            continue
        shell = step.get("shell", "bash")
        if shell not in ("bash", "sh"):
            # pwsh/python bodies are not checked here; this is a syntax gate for
            # the shell scripts, not a general linter.
            continue
        result = subprocess.run(
            ["bash", "-n"], input=body, capture_output=True, text=True
        )
        if result.returncode != 0:
            name = step.get("name") or f"steps[{index}]"
            detail = (result.stderr or "").strip().splitlines()
            problems.append(
                f"{path}:{job_name}:{name}: bash syntax error: "
                + (detail[-1] if detail else "unknown")
            )


def remote_ref_exists(repo, ref):
    url = f"https://api.github.com/repos/{repo}/git/ref/tags/{ref}"
    request = urllib.request.Request(url, headers={"Accept": "application/vnd.github+json"})
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if token:
        request.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            return response.status == 200
    except urllib.error.HTTPError as error:
        if error.code != 404:
            raise
        # Not a tag; it may be a branch (e.g. dtolnay/rust-toolchain@stable).
        branch = f"https://api.github.com/repos/{repo}/git/ref/heads/{ref}"
        branch_request = urllib.request.Request(branch)
        if token:
            branch_request.add_header("Authorization", f"Bearer {token}")
        try:
            with urllib.request.urlopen(branch_request, timeout=20) as response:
                return response.status == 200
        except urllib.error.HTTPError as branch_error:
            if branch_error.code == 404:
                return False
            raise


def check_action_refs(paths, problems):
    """Every `uses:` must resolve. A typo fails the job at startup."""
    refs = set()
    for path in paths:
        with open(path, encoding="utf-8") as handle:
            for line in handle:
                match = USES_RE.match(line)
                if not match:
                    continue
                value = match.group(1).strip("'\"")
                if value.startswith("./"):
                    continue  # local composite action
                if "@" not in value:
                    problems.append(f"{path}: `uses: {value}` has no @ref.")
                    continue
                refs.add(value)

    cache = {}
    for value in sorted(refs):
        repo, ref = value.split("@", 1)
        try:
            exists = remote_ref_exists(repo, ref)
        except Exception as error:  # noqa: BLE001 - network errors are not defects
            print(f"skip: {value} (could not query: {error})", file=sys.stderr)
            continue
        cache[value] = exists
        if not exists:
            problems.append(f"uses: {value} does not resolve upstream.")
    return cache


def main(argv: list[str]) -> int:
    remote = "--remote" in argv[1:]
    try:
        os.chdir(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    except OSError as error:
        print(f"error: cannot locate repository root: {error}", file=sys.stderr)
        return 2

    yaml = load_yaml()
    if yaml is None:
        print("error: PyYAML is required (pip install pyyaml)", file=sys.stderr)
        return 2

    paths = sorted(glob.glob(WORKFLOW_GLOB))
    if not paths:
        print(f"error: no workflows matched {WORKFLOW_GLOB}", file=sys.stderr)
        return 2

    problems: list[str] = []
    for path in paths:
        document = check_yaml(yaml, path, problems)
        if document is None:
            continue
        for job_name, steps in workflow_steps(document):
            check_secret_in_if(path, job_name, steps, problems)
            check_step_references(path, job_name, steps, problems)
            check_unique_ids(path, job_name, steps, problems)
            check_shell_bodies(path, job_name, steps, problems)

    if remote:
        check_action_refs(paths, problems)

    if problems:
        print(f"FAIL: {len(problems)} problem(s)\n", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        return 1

    suffix = " (including remote action refs)" if remote else ""
    print(f"OK: {len(paths)} workflow file(s) validated{suffix}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))