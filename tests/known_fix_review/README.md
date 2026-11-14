# Published fixed-bug review calibration

This case measures explanation and evidence quality for one already disclosed,
fixed issue. It does not measure discovery of unknown vulnerabilities, establish
RCE capability, or support comparisons with XBOW or other agents.

The Bitcoin Core advisory for CVE-2024-35202 documents a remote crash/denial of
service fixed in v25.0 and links upstream PR #26898. The case pins the merge
commit and its first parent and includes only license/C++ source files.

Sources:
- https://bitcoincore.org/en/2024/10/08/disclose-blocktxn-crash/
- https://github.com/bitcoin/bitcoin/pull/26898

Prepare a fresh fixture (the output must not exist):

```powershell
python scripts/prepare_known_fix_review.py --output "$env:TEMP/alphacode-known-fix-review"
```

Read the generated TASK.md. Give each agent a fresh copy of the same source
snapshots and the same model/tool settings. Retain provenance.json, agent logs,
model/runtime versions, elapsed time, tool failures, and review.md. Never treat
a zero exit status as a verified finding.

An independent reviewer applies the manifest's 10-point rubric. An RCE claim
contradicts the documented impact and receives no impact credit. Check source
citations and the patch explanation manually; keyword matching cannot validate
causal reasoning. Report each rubric dimension, false/unsupported claims, and
incomplete runs separately. A score applies only to this case, not all tasks.

The snapshots are for static comparison only. Do not run nodes or create network
payloads. Snapshot preparation downloads only the pinned public source files;
review itself uses local files. Generated snapshots and run outputs belong
outside the repository.
