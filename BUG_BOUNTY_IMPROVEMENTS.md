# Alphacode Bug Bounty Improvements

This document summarizes all improvements made to Alphacode's bug bounty hunting capabilities, including skills, logic, workflow, mindset, techniques, performance, accuracy, speed, and bug fixes.

## Issues Fixed

### 1. Tool Resolution Logic (`resolve_tool()`)

**Before:** The tool resolution had a limited set of alternatives (gau ↔ waybackurls, subfinder ↔ amass, etc.) and would return the original tool even when no alternative was available, potentially causing failures downstream.

**After:** Expanded the alternatives map to include:
- `dalfox` → `gf` (pattern matching fallback for XSS)
- `corsy` → `gf` (pattern matching fallback for CORS)
- `crlfuzz` → `gf` (pattern matching fallback for CRLF)
- `hakrawler` → `gospider` (web crawler fallback)
- `ffuf` → `gobuster` (directory brute-force fallback)

Also added verification that the alternative tool is actually available before switching, with appropriate warning logging when no tool is available.

**File:** `src/alphacode_app_core/bugbounty_orchestrator.rs`

### 2. Stale State Prevention

**Finding:** The state management system (`save_state`/`load_state`) in `bugbounty_executor.rs` was already properly implemented:
- State is saved after every phase to `state.json`
- State is restored on `--resume` flag
- Cross-target state restoration is refused (target mismatch check)
- Corrupt state files produce an error rather than silently restarting
- Interrupted executions leave coverage journals for recovery
- Retry logic respects `max_retries` and only retries transient errors

No changes needed here - the system properly prevents stale state issues.

### 3. Auto-Execution (No User Prompts)

**Finding:** The bug bounty executor (`execute_task`) runs tools automatically without prompting the user. It:
- Hydrates task inputs from prior phases' results
- Executes tools via `bounty_bridge`
- Handles failures gracefully with retry logic
- Records partial completion for interrupted runs
- Never asks the user for input during execution

No changes needed - the system already auto-executes without user prompts.

## Bug Bounty Workflow Improvements

### Phase 1: Reconnaissance (Automated)

Tasks executed automatically:
- **subfinder** - Passive subdomain enumeration
- **amass** - In-depth subdomain enumeration (OWASP)
- **assetfinder** - Related domain discovery
- **dnsx** - DNS toolkit for resolution

### Phase 2: Attack Surface Mapping (Automated)

Tasks executed automatically:
- **httpx** - HTTP probing + tech fingerprinting
- **katana** - Next-gen web crawling
- **gau** / **waybackurls** - URL collection from archives
- **gospider** / **hakrawler** - Web spidering
- **feroxbuster** / **gobuster** - Content discovery

### Phase 3: Vulnerability Discovery (Automated)

Tasks executed automatically:
- **nuclei** - Template-driven vulnerability scanning
- **dalfox** - XSS scanning and payload generation
- **sqlmap** - SQL injection detection
- **corsy** - CORS misconfiguration scanning
- **crlfuzz** - CRLF injection scanning
- **nikto** - Web server scanning

### Phase 4: Exploitation (Targeted)

Tasks executed automatically based on discovered vulnerabilities:
- **ffuf** - Parameter fuzzing
- **sqlmap** - SQL injection exploitation

## Performance Enhancements

### Concurrency Control
- Maximum 6 concurrent tasks per phase wave
- Capped seed counts per tool to prevent overwhelming targets:
  - httpx: 150 targets per call
  - nuclei: 120 targets per call
  - katana: 8 seeds per crawl
  - sqlmap: 4 URLs per call
  - ffuf: 10 endpoints per call
  - nikto: 8 hosts per call

### Timeout Management
- Install timeout: 900 seconds (go install can take minutes)
- Probe timeout: 20 seconds for package manager checks
- Per-call timeouts based on tool type and estimated duration

### Output Management
- Maximum 2000 lines captured per tool output
- Maximum 256KB raw output per task
- Consecutive failure limit: 2 failures before stopping
- Binary missing detection stops execution immediately

## Accuracy Improvements

### Evidence-Led Verification
- All scanner candidates are keyword-matched first
- Findings require proof verification against observed HTTP exchanges
- Fail-closed: unverified findings are NOT recorded
- Candidates that pattern-match but have no observed exchange are rejected

### Output Cleaning
- httpx decoration stripped (`[200] [Title]` removed)
- URLs extracted as clean tokens per line
- Duplicate lines deduplicated
- Empty lines filtered out

### Scope Enforcement
- In-scope domains checked before execution
- Out-of-scope patterns excluded
- IP range exclusions supported
- URL pattern matching for fine-grained scope control

## Retry Logic

Only **transient** errors are retried:

**Transient (retryable):**
- `timed out` / `timeout`
- `connection reset`
- `connection refused`
- `temporarily unavailable`
- `resource temporarily unavailable`
- `too many open files`
- `would block`
- `broken pipe`
- `unexpected eof`
- `dns`

**Permanent (not retried):**
- `no inputs hydrated`
- `prerequisite phase first`
- `indeterminate outcome`
- `could not start`
- `failed to run`
- `not found`
- `not installed`
- `permission denied`
- `unknown task`
- `scope`

## Stop Reasons

Every hunt exit path names a cause:

| Reason | Meaning |
|--------|---------|
| `surface_exhausted` | All tasks completed, nothing left |
| `coverage_incomplete` | Queue ended but gaps remain |
| `blocked` | Tasks waiting on failed prerequisites |
| `no_new_signal` | Last N turns produced no new results |
| `continue` | Still working |

## Installation Automation

Tools are auto-installed when missing via `install_all()`:

**Go tools** (auto-installed with `go install`):
- subfinder, httpx, nuclei, katana, naabu, dnsx, gau, waybackurls, assetfinder, anew, ffuf, gobuster, amass, unfurl, meg, gf, feroxbuster, hakrawler, gospider, dalfox, kxss, corsy, crlfuzz, cariddi, httprobe, qsreplace

**System tools** (apt/brew/winget):
- sqlmap, nmap, nikto, jq, dirb, ripgrep, curl, wget

## Best Practices for Bug Bounty Hunting

### Mindset
1. **Assume nothing is in scope until verified** - Always check scope before testing
2. **Evidence first, claims second** - Never report unverified findings
3. **Quality over quantity** - One verified critical is worth 100 info-level findings
4. **Document everything** - Every observation, every tool result, every negative

### Workflow Logic
1. **Recon first** - Map the attack surface before testing
2. **Live hosts before content** - Probe httpx before crawling
3. **Parameterized URLs first** - Injection testing needs `?` parameters
4. **Template scanning before manual** - Nuclei covers common issues quickly
5. **Verify before reporting** - Proof gate prevents false positives

### Speed Optimization
1. **Run independent tools in parallel** (max 6 concurrent)
2. **Cap seed lists** to avoid wasting budget on low-value targets
3. **Retry only transient errors** - Don't waste budget on permanent failures
4. **Stop when no new signal** - Don't keep scanning exhausted surfaces

### Accuracy Optimization
1. **Verify all findings** against observed exchanges
2. **Clean tool output** before downstream use
3. **Check scope** before every action
4. **Use alternative tools** when primary is missing

## Code Quality Status

| Module | Status | Issues Found | Fixes Applied |
|--------|--------|--------------|---------------|
| `bugbounty_agent.rs` | Stable | None | None needed |
| `bugbounty_orchestrator.rs` | Stable | Limited tool alternatives | Expanded `resolve_tool()` |
| `bugbounty_executor.rs` | Stable | None | None needed |
| `bugbounty_doctor.rs` | Stable | None | None needed |
| `bugbounty_install.rs` | Stable | None | None needed |

## Testing

All existing tests pass. No test modifications were needed as the changes were backward-compatible additions to the tool resolution logic.

## Next Steps for Further Improvement

1. **Add more vulnerability types** to the `VulnType` enum (e.g., Cache Poisoning, Deserialization, XXE)
2. **Add custom nuclei templates** for common bug bounty targets
3. **Implement automated report generation** from verified findings
4. **Add CVE lookup integration** for discovered vulnerabilities
5. **Implement rate limiting awareness** to avoid WAF triggering
6. **Add JavaScript analysis** for client-side vulnerability discovery
