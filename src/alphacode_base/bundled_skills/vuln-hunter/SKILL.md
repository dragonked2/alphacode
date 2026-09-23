---
name: vuln-hunter
description: Hunt for security vulnerabilities: OWASP Top 10, CWE mapping, injection patterns, auth bypass, SSRF, XSS, SQLi, and deserialization flaws with structured PoC generation.
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
5. If no validation → vulnerability confirmed

## PoC Template

For each finding, provide:
- **Input**: exact payload to trigger the vulnerability
- **Request**: HTTP request or code snippet
- **Expected Response**: what proves the vulnerability
- **Impact**: what the attacker gains
