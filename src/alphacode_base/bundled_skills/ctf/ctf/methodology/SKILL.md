# CTF Methodology Skill

## Core Principle: Speed Through Pattern Recognition
Every challenge you solve teaches you a pattern. Every writeup you read teaches you a shortcut. Build a mental database of patterns and apply them instantly.

## Speed-First Methodology

### Phase 0: Pre-Competition Setup (Before CTF starts)
```bash
# Create workspace structure
mkdir -p ~/ctf/{web,crypto,pwn,rev,forensics,misc,scripts,notes,dfir,malware,siem}
cd ~/ctf

# Install/update tools
pip install pwntools pycryptodome z3-solver angr capstone keystone
apt install steghide binwalk foremost exiftool
apt install sqlmap nikto gobuster ffuf hydra john hashcat
apt install tshark wireshark nmap netcat

# Prepare templates
cat > scripts/exploit_template.py << 'EOF'
from pwn import *
context.arch = 'amd64'
context.log_level = 'debug'
p = remote('HOST', PORT)
# exploit here
p.interactive()
EOF

cat > scripts/web_template.sh << 'EOF'
#!/bin/bash
URL=$1
echo "=== Headers ===" && curl -sI $URL
echo "=== robots ===" && curl -s $URL/robots.txt
echo "=== common ===" && for f in flag flag.txt admin .git/config; do
  curl -s -o /dev/null -w "%{http_code} $f\n" $URL/$f
done
EOF
chmod +x scripts/web_template.sh
```

### Phase 1: Rapid Triage (first 5 minutes)
```
Goal: Categorize ALL challenges, solve any quick wins

1. LIST all challenges with scores
2. DOWNLOAD all challenge files to appropriate category folders
3. RUN quick-win scan on everything:
   - grep -rli 'flag{' . 
   - strings * | grep -i 'flag\|ctf'
   - file * on all binaries
4. SOLVE any base64, ROT13, hex, XOR-with-obvious-key challenges
5. SUBMIT all quick flags
6. ASSIGN team members to categories
```

### Phase 2: Systematic Solving (5-60 minutes)
```
For each category, work challenges in order:
1. Lowest points first (usually simpler)
2. Most solves first (pattern is probably common)
3. Challenges matching your known patterns

CHECKPOINT EVERY 15 MINUTES:
- Are we stuck on anything? → Buy hint or move on
- New challenges appeared? → Quick triage
- Scoreboard position? → Adjust strategy
```

### Phase 3: Endgame (last hour)
```
1. Focus on highest-value unsolved challenges
2. Buy ALL remaining hints (points don't matter at end)
3. Submit any partial flags or known patterns
4. Share all discoveries between team members
```

## CTFd API Integration

### Automated Challenge Discovery
```python
import requests
import json

class CTFdClient:
    def __init__(self, base_url, token=None):
        self.base = f"{base_url}/api/v1"
        self.headers = {}
        if token:
            self.headers["Authorization"] = f"Bearer {token}"
    
    def get_challenges(self):
        r = requests.get(f"{self.base}/challenges", headers=self.headers)
        return r.json()["data"]
    
    def get_challenge(self, chal_id):
        r = requests.get(f"{self.base}/challenges/{chal_id}", headers=self.headers)
        return r.json()["data"]
    
    def submit_flag(self, chal_id, flag):
        r = requests.post(f"{self.base}/challenges/{chal_id}/attempt",
                         headers=self.headers,
                         json={"submission": flag})
        return r.json()
    
    def get_hints(self, chal_id):
        r = requests.get(f"{self.base}/challenges/{chal_id}/hints", headers=self.headers)
        return r.json()["data"]
    
    def unlock_hint(self, hint_id):
        r = requests.post(f"{self.base}/hints/{hint_id}/attempt", headers=self.headers)
        return r.json()
    
    def get_scoreboard(self):
        r = requests.get(f"{self.base}/scoreboard", headers=self.headers)
        return r.json()["data"]

# Usage
ctf = CTFdClient("https://ctf.example.com", "your-token-here")
challenges = ctf.get_challenges()

# Sort by: least solves (first blood opportunity) OR lowest points (easier)
unsolved = [c for c in challenges if not c["solved_by_me"]]
unsolved.sort(key=lambda c: (c["solves"], c["value"]))
```

### Challenge Scoring Intelligence
```
Dynamic Scoring: Points = base / (1 + solves * decay)
- First blood: Usually +50-100 bonus points
- Few solves: Challenge is likely hard OR undiscovered
- Many solves: Pattern is probably simple

Strategy:
1. First 30 min: Focus on challenges with 0 solves (first blood)
2. 30-60 min: Focus on challenges with 1-5 solves (still high value)
3. After 60 min: Focus on easiest unsolved (maximize flag count)
```

### Automated File Download
```python
import os

def download_challenge_files(challenge, client):
    chal_dir = f"ctf/{challenge['category']}/{challenge['id']}_{challenge['name']}"
    os.makedirs(chal_dir, exist_ok=True)
    
    for file_info in challenge.get("files", []):
        file_url = f"{client.base}/files/{file_info['id']}/{file_info['name']}"
        r = requests.get(file_url, headers=client.headers)
        filepath = os.path.join(chal_dir, file_info['name'])
        with open(filepath, 'wb') as f:
            f.write(r.content)
        print(f"  Downloaded: {file_info['name']}")
    
    return chal_dir
```

## Pattern Matching from Writeups

### Pattern Recognition Framework
```python
PATTERNS = {
    "base64": {
        "detect": lambda s: "==" in s or (len(s) % 4 == 0 and all(c in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=" for c in s[:50])),
        "solve": lambda s: base64.b64decode(s).decode(),
        "speed": "<1 min"
    },
    "rot13": {
        "detect": lambda s: all(c in "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz" for c in s[:20]) and frequency_analysis(s)[:1] == ['e'],
        "solve": lambda s: codecs.encode(s, 'rot_13'),
        "speed": "<1 min"
    },
    "xor_single": {
        "detect": lambda s: len(s) < 100 and not is_printable(xor_with_byte(s[:20], 0x42)),
        "solve": lambda s: brute_xor_single_byte(s),
        "speed": "<2 min"
    },
    "sql_injection": {
        "detect": lambda r: "error" in r.text.lower() or "syntax" in r.text.lower() or r.status_code == 500,
        "solve": "sqlmap or manual injection",
        "speed": "5-10 min"
    },
    "idor": {
        "detect": lambda url: re.search(r'/api/\w+/\d+', url) or re.search(r'/user/\d+', url),
        "solve": "Increment IDs, try different user IDs",
        "speed": "2-5 min"
    },
    "lfi": {
        "detect": lambda params: any(p.startswith('../') or 'etc/passwd' in p for p in params.values()),
        "solve": "Try ../../etc/passwd, php://filter/convert.base64-encode/resource=index.php",
        "speed": "3-5 min"
    }
}
```

### Writeup Database Search
```bash
# When stuck, search for similar challenges
CHAL_KEYWORDS="login form SQL injection"

# Search CTFtime writeups
curl -s "https://ctftime.org/writeups/?q=$CHAL_KEYWORDS" | grep -oP 'href="/writeups/\d+"' | head -5

# Search GitHub writeups
curl -s "https://api.github.com/search/repositories?q=CTF+writeups+$CHAL_KEYWORDS" | jq '.items[].html_url'

# Search specific CTF writeups
find ~/ctf-writeups -name "*.md" | xargs grep -l "$CHAL_KEYWORDS" 2>/dev/null | head -5
```

## Solving Speed Optimization

### Pre-Computed Solutions
Maintain a library of solve scripts for common patterns:

```bash
~/ctf/scripts/
├── web/
│   ├── sqli_login.sh          # SQL injection on login forms
│   ├── idor.sh                # IDOR enumeration
│   ├── lfi.sh                 # LFI traversal
│   ├── xss_stored.sh          # Stored XSS
│   └── file_upload.sh         # Bypass upload restrictions
├── crypto/
│   ├── xor_brute.py           # Single-byte XOR brute force
│   ├── rsa_common_modulus.py   # RSA common modulus attack
│   ├── rsa_wiener.py           # Wiener's attack
│   ├── des_weak_key.py         # DES weak key detection
│   └── aes_ecb_oracle.py       # ECB byte flipping
├── pwn/
│   ├── ret2win.py             # Standard ret2win
│   ├── ret2libc.py            # ret2libc
│   ├── format_string.py       # Format string exploit
│   ├── rop_gadget.py          # ROP chain builder
│   └── heap_fastbin.py        # Fastbin attack
├── rev/
│   ├── decompile.py           # Auto-decompile and search
│   ├── angr_solve.py          # Symbolic execution
│   └── z3_solver.py           # Constraint solving
├── forensics/
│   ├── extract_all.sh          # Extract from all file types
│   └── network_extract.sh      # Extract files from pcap
├── dfir/
│   ├── volatility_quick.sh     # Quick memory analysis
│   ├── eventlog_triage.sh     # Event log quick analysis
│   └── timeline_builder.sh    # Build forensic timeline
├── malware/
│   ├── static_analysis.sh     # Quick static analysis
│   ├── ioc_extract.sh         # Extract IOCs
│   └── yara_scan.sh           # YARA rule scanning
└── siem/
    ├── log_triage.sh           # Quick log analysis
    └── event_frequency.sh      # Event ID frequency
```

### Speed Hacks
```
1. PARALLEL PROCESSING: Run multiple solves simultaneously
   - Web challenges: Use curl in background
   - Crypto: Try multiple attacks in parallel
   - Binary: Run local and remote attempts simultaneously

2. COPY-PASTE READY: Keep common commands in clipboard
   - Base64 decode: echo "..." | base64 -d
   - Hex decode: echo "..." | xxd -r -p
   - URL decode: python3 -c "import urllib.parse; print(urllib.parse.unquote('...'))"

3. AUTOMATED SUBMISSION: Submit flag immediately on discovery
   - Don't wait for "completion" - partial flags might be valid
   - Some CTFs have flag in multiple formats

4. TOOL CHAINS: Pipe tools together
   - strings binary | grep flag | base64 -d
   - binwalk -e file && exiftool extracted/*
   - tshark -r capture.pcap -T fields -d tcp.port==80,http -e http.file_data
```

## Flag Extraction Patterns

### Automated Flag Search
```bash
#!/bin/bash
find . -type f | while read f; do
    # Check for flag patterns
    grep -Po 'flag\{[^}]+\}|CTF\{[^}]+\}|FLAG\{[^}]+\}' "$f" 2>/dev/null
    
    # Check for base64 encoded flags
    strings "$f" | grep -Ei '[A-Za-z0-9+/]{20,}={0,2}' | while read line; do
        echo "$line" | base64 -d 2>/dev/null | grep -qi flag && echo "B64: $line"
    done
    
    # Check for hex encoded flags
    strings "$f" | grep -Ei '^[0-9a-f]{20,}$' | while read line; do
        echo "$line" | xxd -r -p 2>/dev/null | grep -qi flag && echo "HEX: $line"
    done
done
```

### Steganography Quick Check
```bash
#!/bin/bash
FILE=$1
echo "=== Exiftool ==="
exiftool "$FILE"
echo "=== Strings ==="
strings -n8 "$FILE" | head -20
echo "=== Binwalk ==="
binwalk "$FILE"
echo "=== Steghide (empty password) ==="
steghide extract -sf "$FILE" -f -p "" 2>/dev/null
echo "=== Zsteg (PNG/BMP) ==="
zsteg "$FILE" 2>/dev/null | head -10
echo "=== Stegsolve check needed"
```

## Scoreboard-Driven Strategy

### Adaptive Strategy Based on Position
```
IF leading (top 3):
  - Maintain lead with safe solves
  - Don't risk on hard challenges unless high points
  - Focus on defense (if attack/defense CTF)

IF mid-pack (4-10):
  - Take calculated risks on high-point challenges
  - Look for unsolved challenges others missed
  - Buy hints to catch up

IF behind (11+):
  - Focus on quick wins to build momentum
  - Look for challenges with few solves (first blood)
  - Buy all hints (points don't matter when behind)
```

### Challenge Selection Heuristics
```
Prioritize in this order:
1. Challenges matching your known patterns (fast solve)
2. Challenges with 0-3 solves (high value per solve)
3. Challenges in your strong category
4. High-point challenges (if you can solve them)
5. Low-point challenges (only if nothing else available)

Avoid:
- Challenges with many solves but you're stuck (pattern doesn't match your skills)
- Challenges requiring specialized knowledge you don't have
- Challenges you've spent >20 minutes on without progress
```

## Communication Protocol

### Team Information Sharing
```
STATUS UPDATE FORMAT:
[TIME] [CATEGORY] Challenge: [NAME] | Points: [X] | Status: [SOLVED/STUCK/IN-PROGRESS] | Notes: [brief description]

FLAG SUBMISSION FORMAT:
[TIME] [YOUR_NAME] submitting FLAG{...} for [CHALLENGE_NAME]

POST-MORTEM FORMAT:
[TIME] [CHALLENGE_NAME] POST-MORTEM:
- What worked: [technique that eventually solved it]
- What didn't work: [failed attempts]
- Time spent: [minutes]
- Pattern: [reusable pattern for similar challenges]
```

### Knowledge Base Building
```
AFTER EACH CHALLENGE:
1. Document the pattern in your solve script comments
2. Add any new tool usage to your cheat sheet
3. Share the pattern with teammates
4. Update your speed template if it was a fast solve

EVERY 5 CHALLENGES:
1. Review what patterns are repeating
2. Create automation for repeated patterns
3. Update your category-specific scripts
4. Share best practices with team
```

## Adaptive Learning Engine

### Pattern Recognition Tracker
```python
# Track solved patterns for adaptive learning
pattern_db = {
    "solved": {},      # pattern_name → {count, avg_time, last_seen}
    "failed": {},      # pattern_name → {count, techniques_tried}
    "speed_records": {} # pattern_name → best_solve_time
}

def record_solve(pattern, time_seconds):
    if pattern not in pattern_db["solved"]:
        pattern_db["solved"][pattern] = {"count": 0, "total_time": 0, "times": []}
    pattern_db["solved"][pattern]["count"] += 1
    pattern_db["solved"][pattern]["total_time"] += time_seconds
    pattern_db["solved"][pattern]["times"].append(time_seconds)
    
    # Update speed record
    if pattern not in pattern_db["speed_records"] or time_seconds < pattern_db["speed_records"][pattern]:
        pattern_db["speed_records"][pattern] = time_seconds

def get_pattern_priority(patterns):
    """Rank patterns by solve probability and speed"""
    scored = []
    for p in patterns:
        if p in pattern_db["solved"]:
            stats = pattern_db["solved"][p]
            avg_time = stats["total_time"] / stats["count"]
            confidence = min(stats["count"] / 5.0, 1.0)  # Max confidence at 5 solves
            score = confidence / (avg_time + 1)  # Higher score = faster + more confident
        else:
            score = 0.5  # Unknown pattern - neutral score
        scored.append((p, score))
    return sorted(scored, key=lambda x: -x[1])
```

### Difficulty Estimation
```python
def estimate_difficulty(challenge):
    """Estimate challenge difficulty based on available metadata"""
    difficulty_signals = []
    
    # Points-based estimation
    if challenge.get("value", 0) > 500:
        difficulty_signals.append("hard")
    elif challenge.get("value", 0) > 200:
        difficulty_signals.append("medium")
    else:
        difficulty_signals.append("easy")
    
    # Solve count estimation
    solves = challenge.get("solves", 0)
    if solves == 0:
        difficulty_signals.append("unsolved")
    elif solves > 100:
        difficulty_signals.append("many_solves")
    elif solves < 5:
        difficulty_signals.append("few_solves")
    
    # Tag-based estimation
    tags = challenge.get("tags", [])
    advanced_tags = ["crypto", "pwn", "rev", "forensics"]
    easy_tags = ["web", "misc", "encoding"]
    
    if any(t in advanced_tags for t in tags):
        difficulty_signals.append("advanced_category")
    if any(t in easy_tags for t in tags):
        difficulty_signals.append("easy_category")
    
    return difficulty_signals
```

---

# Multi-Layer Challenge Decomposition

## Layer Identification Framework

```
FOR COMPLEX CHALLENGES, decompose into layers:

LAYER 0: META-LAYER
- What is the challenge ABOUT (not what it asks)?
- Is the challenge testing knowledge, skill, or awareness?
- What domain knowledge does the author assume?

LAYER 1: SURFACE LAYER
- What is the obvious entry point?
- What does the description explicitly ask for?
- What tools would a novice use?

LAYER 2: HIDDEN LAYER
- What is NOT mentioned in the description?
- What would be visible if you looked at the data differently?
- What encoding/encryption is applied?

LAYER 3: TRAP LAYER
- What is the author trying to make you miss?
- What is the most likely mistake?
- What would happen if you submitted the first result?

LAYER 4: META-TRAP LAYER
- Is the author expecting you to look for traps?
- Is the obvious trap actually the solution?
- Is there a trap about the trap? (Turtles all the way down)
```

## Multi-Path Solving Strategy

```
WHEN STUCK, apply multi-path strategy:

PATH A: Direct approach (what the challenge asks)
PATH B: Inverse approach (what the challenge prevents)
PATH C: Lateral approach (completely different angle)
PATH D: Meta approach (analyze the challenge structure itself)
PATH E: Social approach (what would the author think is clever?)

Execute paths in order, but switch if:
- Path has no progress after 5 minutes
- Path produces results that seem "too easy"
- Path contradicts information from another path
- Path leads to a trap (honeypot flag, misdirection)
```

## Confidence-Based Action Matrix

```
CONFIDENCE → ACTION:

HIGH confidence + HIGH points → Submit immediately
HIGH confidence + LOW points → Submit, move to higher value
MEDIUM confidence + HIGH points → Verify once, then submit
MEDIUM confidence + LOW points → Submit if time-constrained
LOW confidence + HIGH points → Reconsider approach, do NOT submit
LOW confidence + LOW points → Abandon or buy hint
VERY LOW confidence → Rethink entire approach

TIME-BASED OVERRIDE:
- <5 min remaining in CTF → Submit MEDIUM+ confidence
- <1 min remaining → Submit LOW+ confidence
- Plenty of time → Only submit HIGH confidence
```

## Challenge Author Profiling

```
PROFILE the challenge author:

SKILL LEVEL:
- Junior: Simple vulnerabilities, obvious patterns, few traps
- Intermediate: Known vulnerabilities with 1-2 traps
- Advanced: Custom vulnerabilities, multiple traps, misdirection
- Expert: Novel techniques, meta-traps, adversarial descriptions

TEACHING STYLE:
- Teacher: Wants you to learn, hints are genuine
- Adversarial: Wants to prevent you from solving, everything is a trap
- Trickster: Mix of teaching and trapping, requires careful reading
- Minimalist: Very little description, challenge speaks for itself

COMMON AUTHOR TRAPS BY LEVEL:
├── Junior: Flag in strings, default creds, obvious SQLi
├── Intermediate: 1 encoding layer, basic misdirection, honeypot flags
├── Advanced: Multi-layer encoding, description traps, meta-challenges
└── Expert: Adversarial descriptions, time-based traps, meta-meta-traps
```

## Counter-Adversarial Strategy

```
AGAINST ADVERSARIAL AUTHORS:

1. ASSUME EVERYTHING IS A TRAP
   - Description text: trap
   - "Helpful" hints: trap
   - Obvious solution: trap
   - Easy find: trap

2. VERIFY EVERYTHING
   - Decode → re-encode → verify
   - Find flag → validate format → cross-check
   - Solve → reproduce → document

3. USE RED TEAM THINKING
   - "If I were the author, where would I hide the real flag?"
   - "What would I punish a lazy solver for?"
   - "What's the one thing a novice would never check?"

4. APPLY PARANOID CHECKLIST
   □ Did I read EVERY word of the description?
   □ Did I check for hidden characters in the description?
   □ Did I consider that the obvious answer is wrong?
   □ Did I look for what's MISSING (not just what's present)?
   □ Did I verify the flag format matches exactly?
   □ Did I consider that finding the flag too easily is a red flag?
```
