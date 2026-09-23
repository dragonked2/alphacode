---
name: evidence-locker
description: Evidence management — Structured proof artifacts, HTTP request/response capture, screenshots, tool output storage, and evidence-to-finding linking. Use when collecting evidence, capturing proof, organizing test results, or linking evidence to findings.
---

# EVIDENCE LOCKER

**Every finding needs proof. Every proof needs a home.**

---

## 1. EVIDENCE TYPES

```
┌─────────────────────────────────────────────────────────┐
│                    EVIDENCE TYPES                       │
├──────────────────┬──────────────────────────────────────┤
│ HTTP Evidence    │ Raw requests, responses, headers     │
│ Tool Output      │ Nuclei, sqlmap, nmap, ffuf results   │
│ Screenshots      │ Visual proof of exploitation         │
│ Code Snippets    │ PoC scripts, exploit code            │
│ Timestamps       │ When evidence was collected          │
│ Hashes           │ Integrity verification               │
└──────────────────┴──────────────────────────────────────┘
```

---

## 2. EVIDENCE TEMPLATE

```markdown
## Evidence: EVD-001

**ID:** EVD-001
**Type:** HTTP Request/Response
**Related Finding:** FND-001 (IDOR)
**Collected:** 2026-01-16 14:32:00 UTC
**Integrity:** sha256:a1b2c3d4...

### Description
HTTP request showing IDOR — accessing another user's profile with attacker's token.

### HTTP Request
```http
GET /api/users/12345 HTTP/1.1
Host: api.example.com
Authorization: Bearer eyJhbGciOiJIUzI1NiIs...
Content-Type: application/json
```

### HTTP Response
```http
HTTP/1.1 200 OK
Content-Type: application/json

{
  "id": 12345,
  "email": "victim@example.com",
  "name": "John Doe",
  "phone": "+1-555-0123",
  "ssn": "123-45-6789"
}
```

### Analysis
- Attacker token belongs to user ID 67890
- Response contains data for user ID 12345
- Sensitive PII exposed (email, phone, SSN)
- Reproducible with different user IDs

### Reproduction Command
```bash
curl -s -H "Authorization: Bearer ATTACKER_TOKEN" \
  "https://api.example.com/api/users/VICTIM_ID" | jq .
```
```

---

## 3. EVIDENCE COLLECTION PATTERNS

### HTTP Request/Response

```bash
# Capture with curl verbose
curl -v -s "https://target.com/api/users/123" \
  -H "Authorization: Bearer TOKEN" 2>&1 | tee evidence/EVD-XXX.txt

# Capture with httpie
http --print=hHbB GET "https://target.com/api/users/123" \
  "Authorization: Bearer TOKEN" > evidence/EVD-XXX.txt

# Capture full request with timestamps
curl -v -s -w "\n--- Timing ---\nDNS: %{time_namelookup}s\nConnect: %{time_connect}s\nTLS: %{time_appconnect}s\nTotal: %{time_total}s\n" \
  "https://target.com/api/users/123" > evidence/EVD-XXX.txt
```

### Tool Output

```bash
# Nuclei with JSON output
nuclei -u https://target.com -t ~/nuclei-templates/ -json -o evidence/nuclei-XXX.json

# sqlmap output
sqlmap -u "https://target.com/?id=1" --batch --output-dir=evidence/sqlmap-XXX/

# nmap output
nmap -sV -sC -oX evidence/nmap-XXX.xml -oN evidence/nmap-XXX.txt target.com

# ffuf output
ffuf -u https://target.com/FUZZ -w wordlist.txt -o evidence/ffuf-XXX.json -of json
```

### Screenshots

```bash
# Using playwright (if available)
npx playwright screenshot "https://target.com/vuln-page" evidence/screenshot-XXX.png

# Using cutycapt (if available)
cutycapt --url="https://target.com/vuln-page" --out=evidence/screenshot-XXX.png

# Using Firefox headless
firefox --headless --screenshot=evidence/screenshot-XXX.png "https://target.com/vuln-page"
```

### PoC Scripts

```bash
# Save PoC as executable script
cat > evidence/poc-XXX.py << 'EOF'
#!/usr/bin/env python3
"""PoC for FND-XXX: [Vuln Type] in [Endpoint]"""
import requests

TARGET = "https://example.com"
ATTACKER_TOKEN = "eyJ..."
VICTIM_ID = 12345

# Step 1: Access victim's data with attacker's token
resp = requests.get(
    f"{TARGET}/api/users/{VICTIM_ID}",
    headers={"Authorization": f"Bearer {ATTACKER_TOKEN}"}
)

print(f"Status: {resp.status_code}")
print(f"Response: {resp.json()}")

# Verify impact
data = resp.json()
assert data["id"] == VICTIM_ID, "IDOR confirmed — accessed different user's data"
print("[+] IDOR CONFIRMED — accessed user data with wrong token")
EOF
chmod +x evidence/poc-XXX.py
```

---

## 4. EVIDENCE STRUCTURE

```
evidence/
├── EVD-001-idor-request.txt      # HTTP request/response
├── EVD-001-idor-screenshot.png   # Visual proof
├── EVD-002-sqli-error.txt        # SQL error message
├── EVD-002-sqli-extract.txt      # Data extraction output
├── EVD-003-xss-poc.html          # XSS proof page
├── poc-001-idor.py               # PoC script
├── poc-002-sqli.py               # PoC script
├── nuclei-results.json           # Scanner output
├── nmap-results.xml              # Port scan
└── MANIFEST.md                   # Evidence index
```

---

## 5. EVIDENCE MANIFEST

Track all evidence in a manifest:

```markdown
## Evidence Manifest — example.com Pentest

| ID | Type | Finding | Collected | Description |
|----|------|---------|-----------|-------------|
| EVD-001 | HTTP | FND-001 (IDOR) | 2026-01-16 14:32 | Accessing victim's profile with attacker token |
| EVD-002 | HTTP | FND-002 (SQLi) | 2026-01-16 15:45 | SQL error on /search endpoint |
| EVD-003 | Tool | FND-002 (SQLi) | 2026-01-16 16:00 | sqlmap data extraction output |
| EVD-004 | Screenshot | FND-003 (XSS) | 2026-01-17 09:15 | XSS alert box execution |
| EVD-005 | HTTP | FND-005 (SSRF) | 2026-01-17 10:30 | Cloud metadata access |

### Integrity Hashes
- EVD-001: sha256:a1b2c3d4...
- EVD-002: sha256:e5f6g7h8...
- EVD-003: sha256:i9j0k1l2...
```

---

## 6. EVIDENCE-TO-FINDING LINKING

Every finding must reference its evidence:

```markdown
## FINDING: IDOR in /api/users/{id}

### Evidence
- **EVD-001**: HTTP request showing access to victim's data (request/response)
- **EVD-002**: Screenshot of admin panel showing user data (visual proof)
- **EVD-003**: PoC script `poc-001-idor.py` (reproduction)

### Evidence Chain
1. EVD-001 proves the vulnerability exists
2. EVD-002 proves the data is sensitive
3. EVD-003 proves it's reproducible
→ All three together = reportable finding
```

---

## 7. EVIDENCE BEST PRACTICES

```
DO:
✓ Collect evidence BEFORE exploiting
✓ Include timestamps in all evidence
✓ Save raw HTTP requests/responses
✓ Hash evidence for integrity verification
✓ Link evidence to findings
✓ Keep evidence organized by finding
✓ Include reproduction steps
✓ Capture both request AND response

DON'T:
✗ Modify evidence after collection
✗ Delete evidence (even for rejected findings)
✗ Store evidence without timestamps
✗ Report findings without evidence
✗ Mix evidence from different findings
✗ Forget to capture headers
```

---

## 8. QUICK COMMANDS

```bash
# Add text evidence
/evidence add "HTTP response shows SQL error: You have an error in your SQL syntax"

# Import file as evidence
/evidence import evidence/captured-response.txt

# List all evidence
/evidence list

# Link evidence to finding
/evidence link EVD-001 FND-001

# Generate evidence manifest
/evidence manifest
```
