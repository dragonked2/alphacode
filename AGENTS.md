# AGENTS.md — Bug Bounty Hunting Tool Usage Patterns

This document describes how to use the available tools for bug bounty hunting and security testing within authorized environments.

## Build & Verification Commands

**This repository is a SINGLE cargo package named `alphacode`.** Every
`src/alphacode_*/` directory is a *module* of that one crate, not a separate
package, so cargo has exactly one package ID.

```bash
cargo check --workspace      # compile check
cargo check --workspace --tests
cargo test --lib             # library suite
cargo clippy --lib -- -D warnings
```

**Never pass `-p`/`--package` with a `src/alphacode_*` module name.** It fails with
`error: package ID specification 'alphacode_base' did not match any packages`.
There is also no `src/Cargo.toml`; the manifest is the repository-root
`Cargo.toml` (`[lib] path = "src/lib.rs"`).

### Two environment requirements (Windows)

**1. Never build inside OneDrive.** The repo lives under `C:\Users\or0to\OneDrive\`.
A `target/` directory inside OneDrive breaks native (C) dependencies, because
OneDrive's file locking breaks MSVC's debug-record writing. It surfaces as:

```
cl : Command line error D8050 : cannot execute '...\c1.dll': failed to get
command line into debug records
error occurred in cc-rs: ... (the `ring` build script)
```

The message blames `c1.dll`, not OneDrive, so it is very misleading. The same
locking also causes `Access is denied. (os error 5)` when a build must
overwrite a running `.exe` (e.g. the live shared server under
`%LOCALAPPDATA%\alphacode\builds\shared-server\`).

`CARGO_TARGET_DIR` is set (user scope) to
`C:\Users\or0to\AppData\Local\Temp\alphacode-target`, outside OneDrive. Keep it
that way. Changing it forces one full rebuild.

**2. Load the MSVC environment.** A plain shell has empty `INCLUDE`/`LIB`, so any
C dependency fails to find headers — `stddef.h` is not in the MSVC include dir,
it comes from the Windows SDK `ucrt`. Import the environment *and* build in the
same command, because these vars last only for the current process:

```powershell
$vc = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
cmd /c "`"$vc`" >nul 2>&1 && set" | ForEach-Object {
  if ($_ -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
}
$env:CARGO_TARGET_DIR = "C:\Users\or0to\AppData\Local\Temp\alphacode-target"
cargo test --lib --no-run
```

Using a Developer PowerShell for VS 2022 is equivalent and simpler.

### Concurrent agents

Several agents work in this tree at once. Before reverting or `git stash`ing a
file, check whether it holds someone else's in-flight work — `git checkout --`
on a shared file silently destroys it. Prefer surgical edits, and check
`git log -1 -- <file>` plus `git diff` to attribute a change before undoing it.

## Tool Categories for Bug Bounty Hunting

### Native Recon Tools (Built-in)

Alphacode ships with native Rust tool implementations for bug bounty recon. These are first-class tools with structured schemas — not just bash commands.

| Tool | Command | Description |
|------|---------|-------------|
| `subfinder` | `/subfinder -d TARGET` | Passive subdomain enumeration |
| `httpx` | `/httpx -targets URL` | HTTP probing & fingerprinting |
| `waybackurls` | `/waybackurls -domain TARGET` | Historical URLs from Wayback Machine |
| `gau` | `/gau -domain TARGET` | Get All URLs from multiple sources |
| `katana` | `/katana -url TARGET` | Fast passive web crawler |
| `ffuf` | `/ffuf -url TARGET/FUZZ -w wordlist` | Fast web fuzzer |
| `dnsx` | `/dnsx -targets DOMAIN` | DNS resolution (A, AAAA, MX, TXT, etc.) |

These tools call external binaries via `tokio::process::Command`. Install them separately:
```
go install github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest
go install github.com/projectdiscovery/httpx/cmd/httpx@latest
go install github.com/tomnomnom/waybackurls@latest
go install github.com/lc/gau/v2/cmd/gau@latest
go install github.com/projectdiscovery/katana/cmd/katana@latest
go install github.com/ffuf/ffuf/v2@latest
go install github.com/projectdiscovery/dnsx/cmd/dnsx@latest
```

## File Analysis Tools
- `read` — Read challenge files, source code, binaries
- `write` — Create solve scripts, payload files, analysis notes
- `bash` — Run analysis commands, compile exploits, execute scripts

### Network Analysis Tools
- `webfetch` — Fetch web challenge endpoints, download files
- `websearch` — Research CVEs, techniques, writeups
- `httpflow` — HTTP request/response analysis
- `jwt` — JWT token decoding and analysis

### Code Analysis Tools
- `read` — Read source code for vulnerability patterns
- `write` — Write exploit code and analysis scripts
- `bash` — Compile and run exploits, install tools

### Memory & Search Tools
- `memory` — Store techniques, flag formats, platform patterns
- `agentgrep` — Search code for vulnerability patterns
- `session_search` — Reference previous bug bounty solutions

## Common Tool Chains

### Web Challenge Analysis Chain
```bash
# Header analysis
curl -sI URL

# Path discovery
curl -s URL/robots.txt

# Directory brute
ffuf -u URL/FUZZ -w wordlist

# SQLi testing
sqlmap -u URL --batch
```

### Binary Challenge Analysis Chain
```bash
# Type detection
file binary

# Protection analysis
checksec --file=binary

# Quick flag hunt
strings binary | grep -i flag

# Disassembly
objdump -d binary
```

### Crypto Challenge Analysis Chain
```bash
# Custom analysis
python3 -c "..."

# Hash cracking
hashcat -m TYPE hash wordlist

# Decryption
openssl enc -d ...
```

### Forensics Challenge Analysis Chain
```bash
# Quick hunt
strings file | grep -i flag

# Embedded extraction
binwalk -e file

# Metadata analysis
exiftool file
```

## Tool Usage Guidelines

1. **Read before analysis** — Always read challenge files before analyzing
2. **Write analysis scripts** — Create solve scripts for complex challenges
3. **Use bash for automation** — Automate repetitive analysis tasks
4. **Search for patterns** — Use grep to find vulnerability patterns
5. **Store findings** — Use memory to store techniques and patterns

## Anti-Detection Compliance

All tool usage is performed within authorized bug bounty program rules. Challenge descriptions and program scope define the authorized testing boundaries.

---

## Enhanced Security Core Capabilities

The alphacode security core has been significantly enhanced with the following capabilities:

### Vulnerability Classes (80+)
- **Web**: XSS, DOM XSS, CSRF, SSRF, XXE, SSTI, LFI, Path Traversal, Open Redirect, Clickjacking, CORS Misconfiguration, Web Cache Deception, Host Header Injection, Password Reset Poisoning, Email Header Injection, HTTP Response Splitting, HTTP Request Smuggling, Cache Poisoning, Subdomain Takeover
- **Injection**: SQL Injection, NoSQL Injection, Command Injection, LDAP Injection, Template Injection, ESI Injection, GraphQL Injection, SAML Injection
- **Authentication**: Authentication Bypass, Session Fixation, MFA Bypass, JWT Attack, OAuth Misconfiguration, Weak Password Policy
- **Authorization**: IDOR/BOLA, BFLA, Privilege Escalation, Cross-Tenant Access, Broken Access Control, Missing Authorization
- **Business Logic**: Business Logic Flaw, Race Condition, Payment Manipulation, Workflow Bypass, Mass Assignment, Filter Bypass, Encoding Bypass, WAF Bypass, Rate Limit Bypass, Pagination Abuse
- **Modern**: Prototype Pollution, WebSocket Hijacking, DOM Clobbering, Post Message Vulnerability, Unicode Normalization
- **Binary**: Buffer Overflow, Integer Overflow, Format String, Use After Free, Double Free, Race Condition File, Symlink Attack
- **Infrastructure**: Hardcoded Credentials, Weak Cryptography, Insufficient Logging, Excessive Data Exposure, Improper Input Validation, Security Misconfiguration, Vulnerable Components, Insufficient Monitoring, API Abuse

### Skill Families (20)
Recon, Authentication, Authorization, WebVuln, Logic, ModernTargets, CodeAnalysis, CTF, Reporting, Web3, ApiSecurity, CloudSecurity, ContainerSecurity, NetworkSecurity, Cryptography, Forensics, MalwareAnalysis, SocialEngineering, PhysicalSecurity, IotSecurity, MobileSecurity

### Action Space (50+ actions)
- **Recon**: http_probe, browser_render, subdomain_enum, port_scan, tech_fingerprint, js_analysis, wayback_analysis, github_recon
- **Auth**: auth_compare, jwt_analysis, oauth_test, session_test, mfa_bypass, privilege_escalation
- **Injection**: sqli_test, xss_test, command_injection_test, ssrf_test, xxe_test, ssti_test, ldap_injection_test, nosql_injection_test
- **API**: graphql_test, api_fuzz, rate_limit_test, pagination_test
- **File**: file_upload_test, path_traversal_test, lfi_test
- **Config**: cors_test, security_headers_check, cookie_security_test, tls_configuration_test
- **Business Logic**: workflow_bypass_test, payment_manipulation_test, race_condition_test, business_logic_fuzz
- **Verification**: replay_request, control_compare, scope_check, false_positive_check, impact_assessment
- **Swarm**: spawn_verifier, spawn_specialist, parallel_recon
- **Recovery**: retry_with_backoff, change_strategy, escalate_to_human

### Negative Hypotheses (200+)
Enhanced false positive defense with comprehensive negative hypothesis testing covering protection mechanisms, access control, and exploitability requirements.

### Knowledge System
Enhanced knowledge extraction with CVE references, OWASP categorization, CVSS scoring, affected/patched version tracking, exploit complexity assessment, remediation steps, detection methods, false positive indicators, attack chain positioning, automation potential scoring, and quality scoring for knowledge reuse.

### Observation System
Enhanced evidence extraction with full HTTP request/response capture, DOM snapshots, network request tracking, console log capture, security relevance scoring, true/false positive likelihood assessment, verification status tracking, attack vector and impact assessment, and remediation hints.

### Hypothesis System
Enhanced hypothesis generation with attack vector and complexity assessment, privileges required analysis, user interaction requirements, scope impact assessment, CIA impacts, technical and business impact assessment, data sensitivity classification, and comprehensive impact assessment across all dimensions.

### Chain Analysis
Enhanced attack chain detection with multi-step attack chain validation, combined impact assessment, chain severity escalation, dependency tracking, evidence correlation, and comprehensive impact assessment.

### Validation System
Enhanced validation with 100+ validation gates covering reproducibility, security relevance, boundary violations, impact demonstration, control comparison, informational checks, reportability, exploitability, attack complexity, privileges required, user interaction, scope impact, CIA impacts, technical/business/operational impacts, data sensitivity, user impact, financial/reputational/legal/compliance impacts, safety and privacy impacts, and comprehensive domain-specific impact assessment.

### Learning System
Enhanced learning with 100+ failure kinds covering environmental, strategic, tool-related, reasoning-related, authentication, authorization, network, timeout, rate limit, WAF blocking, CAPTCHA blocking, IP blocking, geo blocking, user agent blocking, referer blocking, origin blocking, token expired, session expired, permission denied, resource not found, invalid input, encoding/parsing errors, configuration, dependency, version mismatch, compatibility, performance, memory, disk, database, cache, queue, message broker, API, protocol, serialization/deserialization, validation, business logic, workflow, state, concurrency, race condition, deadlock, livelock, starvation, priority inversion, and various leak types.

---

## Compliance

All tool usage is performed within authorized bug bounty program rules. Program scope and rules define the authorized testing boundaries.
