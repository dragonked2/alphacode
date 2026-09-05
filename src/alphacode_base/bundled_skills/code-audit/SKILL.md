---
name: code-audit
description: Deep code audit for vulnerabilities: taint analysis, data flow tracing, unsafe patterns, race conditions, and memory safety issues.
---

# Code Audit Skill

Deep source code security review.

## Taint Analysis Process

1. **Source Inventory** — all entry points where external data enters
2. **Sink Inventory** — all dangerous operations (exec, query, render, deserialize)
3. **Flow Tracing** — follow data from source to sink through all transformations
4. **Validation Check** — is there adequate sanitization at each sink?
5. **Exploitability** — can an attacker control the tainted data?

## Language-Specific Patterns

### Rust
- `unsafe` blocks: audit every one for correctness
- `unwrap()` in production paths: can panic on bad input
- `transmute`: type confusion, undefined behavior
- `String::from_utf8_unchecked`: invalid UTF-8 can crash later

### JavaScript/TypeScript
- `eval()`, `Function()`: code injection
- `innerHTML`, `document.write`: XSS
- `child_process.exec`: command injection
- Prototype pollution via `__proto__`

### Python
- `pickle.loads`: arbitrary code execution
- `os.system`, `subprocess.call(shell=True)`: command injection
- `yaml.load` without `Loader=SafeLoader`: deserialization
- SQL string formatting: SQL injection

## Race Conditions

- TOCTOU: check-then-act on shared state
- Double-spend: concurrent requests exploiting same resource
- Missing locks on shared mutable state
- Non-atomic read-modify-write sequences
