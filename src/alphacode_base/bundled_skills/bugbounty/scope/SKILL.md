---
name: scope
description: Engagement scope discipline — live IN-SCOPE / OUT-OF-SCOPE document created at assessment start and checked before every test. Use when starting any security assessment, before touching a new subdomain or endpoint, or when the user lists exclusions.
---

# SCOPE — NEVER TEST WHAT IS OUT OF SCOPE

**An out-of-scope request is a failed engagement, no matter what you find.**

---

## 1. CREATE THE SCOPE FILE FIRST

Before ANY scan, probe, or request against the target, write the scope file.
No scope file → no testing. No exceptions.

```markdown
## Scope — <target> assessment (<date>)

IN-SCOPE:
- www.example.com
- api.example.com

OUT-OF-SCOPE (never touch):
- help.example.com
- status.example.com

RULES:
- Severity focus: Critical only (or: all severities)
- Forbidden: DoS, spam, social engineering, data destruction
```

Keep it as the first section of your running assessment notes so it
survives compaction (see runbook checkpointing).

---

## 2. CHECK BEFORE EVERY TEST

Before each tool call against a new host, path prefix, or subdomain:

1. Parse the actual destination host, scheme, port and normalized path. Domain
   text inside a path, query or userinfo is not the destination.
2. Match the policy's precise asset semantics: exact hosts, explicit wildcards,
   IP/CIDR ranges and URL restrictions. Exclusions take precedence. A bare host
   listed by a program does not imply permission for every subdomain.
3. Excluded or unmatched destination -> skip it and record why. Resolve ambiguity
   from the official policy or the user while continuing independent work.
4. Recheck redirects and discovered destinations before following or testing them;
   never forward credentials to another origin merely because it was linked.

Filter discovery output through the actual allowlist AND exclusions before DNS
resolution or probing. Removing a few excluded names with grep is insufficient:
it leaves every unknown destination eligible. Preserve skipped discoveries as
inventory entries with a scope decision, not as targets to try later by default.

---

## 3. SCOPE VIOLATIONS

If you catch yourself testing something excluded:

1. Stop that line of testing immediately.
2. Log it in the assessment notes: what was touched, with what tool.
3. Re-check the scope file before continuing.
4. Do not include out-of-scope results in findings.

---

## 4. SCOPE CHANGES MID-ASSESSMENT

- Only the user (or program policy) changes scope — never infer it.
- When scope changes, update the scope file first, then continue.
- Re-filter any queued targets against the updated file.
