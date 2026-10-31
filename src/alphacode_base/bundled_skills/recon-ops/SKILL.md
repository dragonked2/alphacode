---
name: recon-ops
description: "Reconnaissance and tool-selection manual: passive-first asset discovery, subdomain and DNS enumeration, liveness probing, historical URL and JavaScript analysis, content and parameter discovery, active-recon discipline, and the full tool-selection matrix. Load at the start of recon or whenever you need to choose the right scanner, prober, or analysis tool."
auto-invoke: false
---

# Reconnaissance and Tool Selection

This manual carries the detailed recon pipeline and tool-selection matrix that used to live in the always-on system prompt. Load it with `skill_manage` (action=load, name=recon-ops) before recon or when choosing tools. The registered first-party tool inventory stays in the master prompt.

# Discovery and Reconnaissance

Use Alphacode's first-party tools — they are registered runtime tools, not abstractions to describe in prose. Recon tools wrap individually installed binaries; check availability with the `doctor` tool and degrade to an installed alternative instead of stopping.

## Core recon tooling

- Subdomain/DNS discovery: `subfinder`, `assetfinder`, `amass`, `dnsx`
- HTTP probing: `httpx`, `httprobe`
- URL history: `gau`, `waybackurls`, `unfurl`
- Crawling/spidering: `katana`, `hakrawler`, `gospider`, `cariddi`, `meg`
- Wordlisting/content discovery: `ffuf`, `feroxbuster`, `gobuster`
- Vulnerability scanning: `nuclei`, `nikto`, `dalfox`, `kxss`, `sqlmap`
- Fuzz/parameter helpers: `gf`, `qsreplace`, `anew`
- Header/CRLF checks: `corsy`, `crlfuzz`
- Network: `nmap`, `naabu`
- Repo search: `alphagrep` (grep/find/outline/trace modes; `grep` and `file_grep` alias to it), `git` via bash
- Fetch/extract/research: `webfetch`, `scrapling`, `browser`, `websearch`

## Passive-first recon pipeline

Run in this order. Stop each phase when the next phase would not materially change the plan.

1. **Scope load.** Read the current program scope, exclusions, and rules. Record them in the research ledger with date and source.
2. **Root asset enumeration.** Identify every in-scope root domain, CIDR, mobile app, and API surface.
3. **Subdomain enumeration.** `subfinder -d <root> -all -recursive` piped into `dnsx -resp -a -cname`. Merge with `assetfinder`, `amass enum -passive`, and CT logs (`crt.sh` via `webfetch` or `curl`).
4. **Liveness probe.** `httpx -l subs.txt -sc -title -td -cl -location -ip` to fingerprint reachable services.
5. **Wayback/historical URLs.** `gau --subs <root> | anew urls.txt` and `waybackurls <root> | anew urls.txt`. Historical URLs frequently expose deleted endpoints, old API versions, and forgotten parameters.
6. **JS analysis.** `katana -jc -kf all -d 3 -u <target> | anew endpoints.txt`. Download JS bundles and run `alphagrep` for `api`, `token`, `secret`, `key`, `endpoint`, `graphql`, `internal`, `admin`, `debug`.
7. **Content discovery.** `ffuf -w <wordlist> -u https://target/FUZZ -mc 200,204,301,302,307,401,403 -ac` on a small high-value wordlist first, then expand.
8. **Parameter discovery.** `arjun` or `ffuf` against parameter wordlists on known endpoints. `gf` patterns: `gf ssrf`, `gf redirect`, `gf sqli`, `gf xss`, `gf idor`.
9. **Technology fingerprint.** Favicon hash (`httpx -favicon`), headers, cookies, error pages, HTML generator tags, response timing.
10. **ASN/CIDR expansion** (if in scope). `nmap` against the in-scope ranges for exposed services.
11. **Secret hunting.** `nuclei -t exposures/ -t misconfiguration/` plus `alphagrep` on JS bundles and GitHub org repos (if in scope for source).

## Active recon discipline

- Prefer targeted tests when they can answer the question faster than broad scans.
- For external assets, verify the relevant scope before active testing.
- Use passive research and local source analysis to make progress when active testing permissions or limits are unclear.
- Do not run enormous scans merely because a tool exists.
- Do not artificially limit a properly scoped investigation when broader reconnaissance is justified and appropriate.
- Rate-limit requests when the target is production. Bounded concurrency, not full-throttle.
- Record tool versions, options, and outputs.

Record:

- Assets discovered.
- Source of discovery.
- Scope status.
- Technology fingerprint.
- Reachability.
- Relevant endpoints.
- Authentication boundaries.
- Tool versions and important options.
- Unresolved questions.

Avoid repeatedly scanning already covered assets without a new hypothesis or reason.

---


---

# Tool Selection Heuristics

Choose tools based on the objective, evidence needed, and runtime constraints.

| Task | Primary tools | Alternatives | Purpose |
|---|---|---|---|
| Endpoint discovery | `katana`, `gau` | `waybackurls`, `hakrawler`, `gospider`, `cariddi` | Discover routes and historical URLs |
| Subdomain discovery | `subfinder` | `amass`, `assetfinder`, `dnsx`, CT logs | Identify relevant assets |
| HTTP probing | `httpx` | `httprobe`, browser | Verify reachable services |
| XSS testing | `dalfox` | `kxss`, browser manual tests | Detect and validate XSS candidates |
| SQL injection | `sqlmap` | Controlled manual probes via bash | Investigate database-input hypotheses |
| SSRF | `httpflow`, controlled callback listener via bash | Browser, manual tests | Verify server-side request behavior |
| IDOR | `httpflow`/browser, two test accounts | API tooling via bash | Differential authorization testing |
| SSTI | Context-specific probes via bash | Manual tests | Identify template evaluation |
| Binary analysis | `ghidra` | `radare2`, `angr` | Reverse engineering |
| Secret scanning | `alphagrep`/bash pattern search | `nuclei` secret templates | Find exposed secrets |
| API discovery | `ffuf`, `gobuster`, `gf` | `nuclei` templates, source review | Identify API parameters and routes |
| JWT analysis | `jwt` | bash (base64, openssl) | Decode, validate, manipulate tokens |
| Request inspect/replay | `httpflow` | browser, bash | Capture, modify, replay requests |
| Differential diff | `httpflow` + `diff` | scripted harness | Baseline vs. modified comparison |
| Repository search | `alphagrep` | bash ripgrep | Trace code paths and callers |
| Concurrency debugging | Runtime traces, bounded tests | Static analysis | Validate lock and task behavior |
| Race testing | HTTP/2 single-packet harness | bash + `curl --http2` | State machine races |
| Build diagnostics | `doctor`, native build tools | bash | Identify missing dependencies |
| Browser testing | `browser` with network inspection | `httpflow` | Reproduce application behavior |

Tool availability, version, installation status, and permissions must be verified rather than assumed.

When multiple tools exist, start with the fastest reliable tool that can answer the current question. Escalate when the evidence requires it.

A scanner finding is a candidate, not a confirmed vulnerability.

## First-party runtime arsenal — AlphaCode's actual power

These are registered runtime tools. Know them and use them instead of narrating alternatives or hand-rolling bash equivalents:

- **Files and code:** `read`, `write`, `edit`, `multiedit`, `patch`, `apply_patch`, `ls`, `alphagrep` (grep, find, outline, trace modes), `bash`, `open`, `clipboard`, `batch` (parallel tool execution), `side_panel`, `todo` (open/update it on every task), `plan` (plan mode for multi-step work).
- **Web and HTTP:** `browser` (JavaScript rendering, sessions, screenshots, interaction), `webfetch`, `websearch`, `scrapling` (structured extraction), `httpflow` (capture, inspect, modify, replay HTTP requests — the closest equivalent to a proxy's Repeater), `unfurl`.
- **Recon and probing (binaries, so variants are individually installed):** `subfinder`, `assetfinder`, `amass`, `dnsx`, `httpx`, `httprobe`, `gau`, `waybackurls`, `katana`, `hakrawler`, `gospider`, `cariddi`, `meg`, `ffuf`, `feroxbuster`, `gobuster`, `gf`, `qsreplace`, `anew`, `kxss`, `dalfox`, `corsy`, `crlfuzz`, `sqlmap`, `nikto`, `nuclei`, `nmap`, `naabu`.
- **Analysis:** `jwt` (decode/validate/manipulate JSON Web Tokens), `ghidra`, `radare2`, `angr` (binary RE).
- **CTF arsenal:** `pwntools`, `binwalk`, `steghide`, `exiftool`, `volatility`, `tshark`, `z3`, `hashcat`, `john`, `flag_scanner`, `challenge_classifier`, `ctf_auto_solver`.
- **Continuity:** `memory` (persistent memory across sessions — store corrections, scope facts, findings, and checkpoints), `session_search` (search past sessions), `conversation_search` (search this session's history), `grep`/`file_grep` aliases of `alphagrep`.
- **Coordination and environment:** `swarm` (spawn and coordinate parallel workers), `bg` (background tasks), `cron`/`schedule` (scheduled ambient runs), `gmail` (authorized email flows: OTP/reset testing through a controlled inbox), `desktop` (desktop UI automation), `doctor` (diagnose the environment and tool availability), `skill_manage` (load a bundled skill's full methodology on demand), `initiative`, `selfdev`/`self_improve`.

Reach for the specialized tool first; use `bash` only when no registered tool covers the need. If a binary-backed tool is missing its executable, fall back per the failure-recovery patterns instead of declaring the capability unavailable.

---
