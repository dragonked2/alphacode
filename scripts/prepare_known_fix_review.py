#!/usr/bin/env python3
"""Prepare pinned, source-only snapshots for published fixed-bug review."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import tempfile
from urllib.request import Request, urlopen


def validate(case):
    if case["repository"] != "https://github.com/bitcoin/bitcoin":
        raise ValueError("This calibration case only supports the pinned Bitcoin repository")
    for key in ("affected_revision", "fixed_revision"):
        if not re.fullmatch(r"[0-9a-f]{40}", case[key]):
            raise ValueError("Revisions must be full commit hashes")
    if not case["paths"] or len(set(case["paths"])) != len(case["paths"]):
        raise ValueError("Source paths must be nonempty and unique")
    for value in case["paths"]:
        path = PurePosixPath(value)
        if path.is_absolute() or ".." in path.parts or "\\" in value or ":" in value:
            raise ValueError("Source paths must be repository-relative")
        if value != "COPYING" and (not value.startswith("src/") or path.suffix not in (".cpp", ".h")):
            raise ValueError("Only license and C++ source files belong in this fixture")


def download(url):
    request = Request(url, headers={"User-Agent": "alphacode-known-fix-review"})
    with urlopen(request, timeout=30) as response:
        content = response.read(2_000_001)
    if len(content) > 2_000_000:
        raise ValueError("Unexpectedly large source file")
    return content


def prepare(manifest, output, fetch=download):
    case = json.loads(Path(manifest).read_text(encoding="utf-8"))
    validate(case)
    output = Path(output).resolve()
    if output.exists():
        raise FileExistsError(f"Refusing to overwrite {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="known-fix-", dir=output.parent) as temporary:
        staging = Path(temporary) / "fixture"
        staging.mkdir()
        provenance = {"case": case["id"], "manifest_sha256": hashlib.sha256(Path(manifest).read_bytes()).hexdigest(), "files": []}
        for label in ("affected", "fixed"):
            revision = case[label + "_revision"]
            for relative in case["paths"]:
                url = f"https://raw.githubusercontent.com/bitcoin/bitcoin/{revision}/{relative}"
                content = fetch(url)
                destination = staging / label / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(content)
                provenance["files"].append({"snapshot": label, "revision": revision, "path": relative, "url": url, "sha256": hashlib.sha256(content).hexdigest()})
        prompt = f"""# Published fixed-bug review calibration

Review only the previously disclosed issue described by {case['advisory']}.
The affected and fixed source snapshots are pinned in provenance.json.
Compare the local source and explain the documented cause, upstream correction,
and supported impact. Cite file paths and line numbers for both revisions.
Write review.md with observations, inferences, and unverified claims separated.
Do not claim a new vulnerability, RCE, broad coverage, or comparative superiority.
These are source-only snapshots: do not build nodes, generate attack traffic,
write exploits, or inspect unrelated code for new vulnerabilities.

This is a known-fix comprehension case, not a blind vulnerability-discovery test.
Record the agent version, model, start/end time, and any tool failures in run.json.
An independent reviewer grades the result using the case manifest outside this
fixture. No automatic semantic success score is inferred from agent exit status.
"""
        (staging / "TASK.md").write_text(prompt, encoding="utf-8")
        (staging / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n", encoding="utf-8")
        staging.rename(output)
    return provenance


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", type=Path, default=Path(__file__).resolve().parents[1] / "tests/known_fix_review/bitcoin-cve-2024-35202.json")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = prepare(args.case, args.output)
    print(json.dumps({"output": str(args.output.resolve()), "case": result["case"], "source_files": len(result["files"])}))


if __name__ == "__main__":
    main()
