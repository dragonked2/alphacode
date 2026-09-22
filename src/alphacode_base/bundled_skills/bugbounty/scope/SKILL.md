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

1. Extract the host being tested.
2. Match it against IN-SCOPE / OUT-OF-SCOPE.
3. OUT-OF-SCOPE match → skip it, note the skip in one line, move on.
4. Ambiguous (new subdomain not on either list) → treat as OUT-OF-SCOPE
   until the user confirms.Akt never "just quickly check" an ambiguous host.

Subdomain enumeration routinely returns excluded hosts (docs, status,
community, help). Filter enumeration output against the scope file
BEFORE resolving or probing:

```bash
# Example: drop excluded hosts right after enumeration
grep -v -E '^(help|explorer|community)\.' subs.txt > subs-in-scope.txt
```

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
