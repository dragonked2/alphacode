---
name: vuln-hunter
description: "Hunt for security vulnerabilities: OWASP Top 10, CWE mapping, injection patterns, auth bypass, SSRF, XSS, SQLi, and deserialization flaws with structured PoC generation."
---

# Vulnerability Hunter Skill

Proactive vulnerability discovery in source code.

## Hunt Checklist

- **Injection**: SQL, NoSQL, OS command, LDAP, XPath
- **XSS**: reflected, stored, DOM-based
- **SSRF**: internal network access, cloud metadata endpoints
- **Auth Bypass**: missing checks, JWT flaws, session fixation
- **IDOR**: accessing other users' resources via predictable IDs
- **Deserialization**: unsafe object injection, prototype pollution
- **Path Traversal**: directory traversal via user input
- **Race Conditions**: TOCTOU, double-spend, concurrent state mutations

## Taint Analysis

1. Identify **sources** (user input, request params, file reads)
2. Identify **sinks** (SQL queries, command execution, template rendering)
3. Trace data flow from source to sink
4. Check if sanitization/validation exists between source and sink
5. Treat a missing check as a lead. Confirm attacker control, a reachable
   unsafe sink, the relevant trust boundary and a demonstrable consequence;
   framework protections, parameterization or other controls may make the
   apparent flow safe. Mark source-only or untested paths accordingly.

For a suspected protected-content disclosure, compare the complete same-object
public preview and tested content fields. Confirm the tested actor received a
specific private value established by an entitled control or owned fixture.
Status codes, lock flags, counts of withheld text, and placeholder dimensions
do not supply that evidence. Without a demonstrated disclosure, record a lead
or a refuted hypothesis and withhold severity scoring.

## PoC Template

For each finding, provide:
- **Input**: exact payload to trigger the vulnerability
- **Request**: HTTP request or code snippet
- **Expected Response**: what proves the vulnerability
- **Impact**: what the attacker gains
