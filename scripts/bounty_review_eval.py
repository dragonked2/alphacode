"""Score independently adjudicated reports from held-out defensive cases.

This reads JSON records locally and makes no network calls. The adjudicator
assigns root-cause IDs only to claims supported by the retained evidence.
"""

import argparse
from collections import Counter
import json
import math
from pathlib import Path
import statistics


def identifier(value):
    if not isinstance(value, str) or not value.strip():
        raise ValueError("Expected a nonempty identifier")
    return value


def accuracy(counts):
    accepted = counts["accepted"]
    submitted = sum(counts[key] for key in ("accepted", "unsupported", "uncertain", "stale"))
    submitted_claims = submitted + counts["duplicates"]
    eligible = accepted + counts["missed"]
    return {
        "precision": accepted / submitted if submitted else None,
        "unique_accepted_per_claim": accepted / submitted_claims if submitted_claims else None,
        "recall": accepted / eligible if eligible else None,
    }


def array(value, field):
    if not isinstance(value, list):
        raise ValueError(f"{field} must be an array")
    return value


def summarize_groups(groups):
    return {
        name: {
            "totals": dict(group["counts"]),
            "statuses": dict(group["statuses"]),
            **accuracy(group["counts"]),
        }
        for name, group in sorted(groups.items())
    }


def score(corpus):
    totals = Counter(accepted=0, unsupported=0, uncertain=0, stale=0, missed=0, duplicates=0)
    statuses = Counter()
    runs = []
    families = {}
    participants = {}
    evidence_times = []
    seen_runs = set()
    for run in array(corpus["runs"], "runs"):
        run_id = identifier(run["run_id"])
        if run_id in seen_runs:
            raise ValueError(f"Duplicate run ID: {run_id}")
        seen_runs.add(run_id)
        for field in ("fixture_revision", "revision", "model", "configuration"):
            identifier(run[field])
        family = identifier(run.get("family", "uncategorized"))
        participant = identifier(run.get("participant", "unspecified"))
        budget = run["budget_seconds"]
        if type(budget) not in (int, float) or not math.isfinite(budget) or budget <= 0:
            raise ValueError(f"{run_id}: budget_seconds must be positive and finite")
        status = run["status"]
        if status not in ("completed", "failed", "cancelled", "timed_out", "incomplete"):
            raise ValueError(f"{run_id}: invalid run status")
        statuses[status] += 1
        elapsed = run.get("seconds_to_reviewed_evidence")
        if elapsed is not None and (
            type(elapsed) not in (int, float) or not math.isfinite(elapsed) or elapsed < 0
        ):
            raise ValueError(f"{run_id}: invalid time to reviewed evidence")
        roots = array(run["expected_root_causes"], f"{run_id}: expected_root_causes")
        expected = {identifier(root) for root in roots}
        if len(expected) != len(roots):
            raise ValueError(f"{run_id}: duplicate expected root cause")
        evidence = {}
        for artifact in array(run.get("reviewed_evidence", []), f"{run_id}: reviewed_evidence"):
            ref = identifier(artifact["ref"])
            if ref in evidence:
                raise ValueError(f"{run_id}: duplicate evidence reference {ref}")
            evidence[ref] = identifier(artifact["fixture_revision"])
        observed = set()
        seen_claims = set()
        counts = Counter(accepted=0, unsupported=0, uncertain=0, stale=0, duplicates=0)
        for claim in array(run["claims"], f"{run_id}: claims"):
            claim_id = identifier(claim["claim_id"])
            if claim_id in seen_claims:
                raise ValueError(f"{run_id}: duplicate claim ID {claim_id}")
            seen_claims.add(claim_id)
            identifier(claim["review_rationale"])
            disposition = claim["disposition"]
            if disposition in ("unsupported", "uncertain"):
                counts[disposition] += 1
                continue
            if disposition != "accepted":
                raise ValueError(f"{run_id}: invalid disposition for {claim_id}")
            root = identifier(claim["root_cause_id"])
            refs = claim["evidence_refs"]
            if root not in expected or not isinstance(refs, list) or not refs:
                raise ValueError(f"{run_id}: accepted claim needs a corpus root and evidence")
            for ref in refs:
                identifier(ref)
                if ref not in evidence:
                    raise ValueError(f"{run_id}: unknown reviewed evidence reference {ref}")
            if any(evidence[ref] != run["fixture_revision"] for ref in refs):
                counts["stale"] += 1
                continue
            counts["duplicates" if root in observed else "accepted"] += 1
            observed.add(root)
        counts["missed"] = len(expected - observed)
        if elapsed is not None:
            if not observed:
                raise ValueError(f"{run_id}: reviewed evidence time requires an accepted claim")
            evidence_times.append(elapsed)
        totals.update(counts)
        for groups, key in ((families, family), (participants, participant)):
            group = groups.setdefault(key, {"counts": Counter(), "statuses": Counter()})
            group["counts"].update(counts)
            group["statuses"][status] += 1
        runs.append({**run, "counts": dict(counts)})
    return {
        "totals": dict(totals),
        "statuses": dict(statuses),
        **accuracy(totals),
        "by_family": summarize_groups(families),
        "by_participant": summarize_groups(participants),
        "time_to_reviewed_evidence": {
            "median_observed_seconds": statistics.median(evidence_times) if evidence_times else None,
            "runs_with_timing": len(evidence_times),
            "runs_without_timing": len(runs) - len(evidence_times),
        },
        "runs": runs,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("corpus", type=Path, help="JSON corpus with independently reviewed runs")
    args = parser.parse_args()
    print(json.dumps(score(json.loads(args.corpus.read_text(encoding="utf-8"))), indent=2))
