# CTF Solver Skill

## Role
Rapid Capture-The-Flag solver. Speed is the primary differentiator. You triage challenges in <60 seconds, route to the correct category, execute optimized solve scripts, and submit flags. You learn from every writeup you read and every challenge you solve.

## Speed-First Workflow

### Phase 1: Rapid Triage (<60 seconds)
```
1. Download challenge files + note CTFd metadata (points, solves, tags)
2. Run: file *, strings -n8, xxd | head -50 on all binaries
3. Hit web endpoints: curl -I, ffuf -mc 200 with tiny wordlist (500 entries)
4. Route to category:
   - Binary → pwn (file ELF/PE → pwn)
   - Encrypted/encoded strings → crypto
   - Obfuscated/compiled code → rev
   - Network capture (.pcap) → forensics
   - Web service → web
   - Unusual format → misc
```

### Phase 2: Quick Win Scan (first 5 minutes)
```
BEFORE deep analysis, check for low-hanging fruit:
- cat flag, cat flag.txt, cat README.md on any provided files
- strings all binaries for "flag{" "CTF{" "FLAG{" patterns
- Check web endpoints for /flag, /admin, /robots.txt, /.git/config
- Run steghide extract -sf image.jpg (password: "")
- Run binwalk -e on any suspicious files
- Check common crypto: ROT13, base64, hex, XOR with 0x42
```

### Phase 3: Solve or Escalate
```
IF quick win found → submit immediately
ELSE → route to specialized subskill (web/crypto/pwn/rev/forensics/misc)
```

## CTFd Platform Integration

### CTFd API Playbook
Most modern CTFs use CTFd. Know these API endpoints:
```bash
# List all challenges
curl -H "Authorization: Bearer $TOKEN" $CTFD_URL/api/v1/challenges

# Get challenge details (hints, tags, solves)
curl -H "Authorization: Bearer $TOKEN" $CTFD_URL/api/v1/challenges/$ID

# Submit flag
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"submission":"FLAG{...}"}' \
  $CTFD_URL/api/v1/challenges/$ID/attempt

# Get scoreboard
curl -H "Authorization: Bearer $TOKEN" $CTFD_URL/api/v1/scoreboard

# Unlock hints (costs points, use wisely)
curl -X POST -H "Authorization: Bearer $TOKEN" \
  $CTFD_URL/api/v1/hints/$HINT_ID/attempt
```

### Team Token Workflow
```bash
# Login and get session token
TOKEN=$(curl -s -X POST -H "Content-Type: application/json" \
  -d '{"name":"TEAM","password":"PASS"}' \
  $CTFD_URL/api/v1/login | jq -r .data.session_token)

# Or use API token from profile
export TOKEN="your-api-token-here"
```

### Challenge Scoring Intelligence
- **Dynamic scoring**: Fewer solves = more points. Prioritize unsolved challenges.
- **First blood bonus**: Extra points for first solve. Race condition matters.
- **Hint cost**: Usually 50-100 points. Only buy hints when stuck >15 minutes.
- **Retired challenges**: Usually already solved. Focus on active challenges.

## Pattern Database (from Writeups)

### Fast-Path Patterns (solve in <5 minutes each)

| Pattern | Detection | Quick Solve |
|---------|-----------|-------------|
| Base64 in flag format | `flag.*base64` | `echo "..." \| base64 -d` |
| ROT13 encoded | Letter frequency uniform | `echo "..." \| tr A-Z N-ZA-M` |
| XOR with single byte | Short encrypted string | Brute force: `for i in $(seq 0 255); do echo -n "..." \| xxd -p \| xxd -r -p \| xorsum -s $i; done` |
| SQL injection in login | Login form | `' OR 1=1--` or `admin'--` |
| Hidden form field | View source | Change value, resubmit |
| Cookie manipulation | DevTools → Application | Change role=user to role=admin |
| Directory listing | URL + / | Browse, find flag.txt |
| robots.txt | URL/robots.txt | Follow Disallow paths |
| Git leak | /.git/config visible | `git-dumper` or `git clone` |
| LFI with /etc/passwd | URL param ?page= | `../../../../etc/passwd` |
| XSS in search | Search input | `<script>alert(1)</script>` → check reflected |
| SSH with default creds | OpenSSH port | `admin:admin`, `root:root`, `ctf:ctf` |
| Web directory brute | Any web service | `ffuf -u $URL/FUZZ -w /usr/share/seclists/Discovery/Web-Content/raft-small-directories.txt` |

### Medium Patterns (5-20 minutes)

| Pattern | Detection | Approach |
|---------|-----------|----------|
| Custom XOR encryption | Multiple encrypted values | Recover key via known plaintext |
| Padding oracle | "Invalid padding" error | Padbuster or custom script |
| SQL injection (blind) | Login works/won't differently | Time-based: `'; IF (1=1) WAITFOR DELAY '0:0:5'--` |
| IDOR in API | /api/users/1, /api/users/2 | Increment IDs, check /api/users/admin |
| File upload bypass | Upload form | Rename .php → .php5, modify Content-Type |
| Race condition | Single-use operations | Send 20+ concurrent requests with curl |

### Slow Patterns (>20 minutes, skip if simpler challenges remain)

| Pattern | When to Attempt |
|---------|-----------------|
| Custom crypto | No known attacks, but points are high |
| Reverse engineering complex binary | No quick strings/pattern match |
| Multi-step exploitation chain | Individual steps found but need chaining |
| Steganography + decryption | Steg content but encoded |

## Automated Solve Scripts

### One-Liner Flag Hunters
```bash
# Find all flags in downloaded files
grep -rnEi 'flag\{[^}]+\}|CTF\{[^}]+\}|FLAG\{[^}]+\}' . 2>/dev/null

# Find flags in base64-encoded strings
strings * | grep -i '[A-Za-z0-9+/]\{20,\}==' | while read s; do
  decoded=$(echo "$s" | base64 -d 2>/dev/null)
  echo "$decoded" | grep -qi flag && echo "FLAG: $decoded"
done

# Find flags in hex strings
strings * | grep -Ei '^[0-9a-f]{20,}$' | while read s; do
  decoded=$(echo "$s" | xxd -r -p 2>/dev/null)
  echo "$decoded" | grep -qi flag && echo "FLAG: $decoded"
done
```

### Quick Web Check Script
```bash
#!/bin/bash
URL=$1
echo "=== Headers ==="
curl -sI $URL
echo "=== robots.txt ==="
curl -s $URL/robots.txt
echo "=== common files ==="
for f in flag flag.txt README.md .git/config admin index.html .env; do
  code=$(curl -s -o /dev/null -w '%{http_code}' $URL/$f)
  echo "$f → $code"
done
echo "=== directory listing ==="
ffuf -mc 200,301,302,403 -u $URL/FUZZ -w /usr/share/seclists/Discovery/Web-Content/common.txt -s 2>/dev/null | head -20
```

### Quick Binary Check
```bash
#!/bin/bash
FILE=$1
echo "=== file type ==="
file $FILE
echo "=== strings (flag patterns) ==="
strings $FILE | grep -iE 'flag\{[^}]+\}|CTF\{[^}]+\}'
echo "=== strings (interesting) ==="
strings -n8 $FILE | head -30
echo "=== imports ==="
objdump -p $FILE 2>/dev/null | grep -i "NEEDED\|dynamic"
echo "=== protections ==="
checksec --file=$FILE 2>/dev/null || readelf -l $FILE | grep GNU_STACK
```

### Quick Crypto Check
```bash
#!/bin/bash
FILE=$1
echo "=== file type ==="
file $FILE
echo "=== entropy (high = encrypted/compressed) ==="
ent $FILE 2>/dev/null || python3 -c "
import math
data=open('$FILE','rb').read()
freq=[data.count(bytes([i]))/len(data) for i in range(256)]
e=-sum(f*math.log2(f) for f in freq if f>0)
print(f'Entropy: {e:.2f} bits/byte (max 8.0)')
"
echo "=== hex dump (first 128 bytes) ==="
xxd -l128 $FILE
```

## Triage Decision Matrix

```
IF file is ELF binary:
  → pwn (check with checksec, run locally)
  
IF web service with login:
  → web (try SQLi, default creds, IDOR)

IF encrypted text (not binary):
  → crypto (check if known cipher, key length)

IF .pcap/.pcapng file:
  → forensics (strings, tshark, extract files)

IF obfuscated code (pyc, class, dex):
  → rev (decompile, analyze logic)

IF unusual file format:
  → misc (research format, try standard tools)

IF multiple files:
  → Try the simplest file first. Often one file is the key.

IF no files, only text description:
  → May be pure logic puzzle. Read carefully.
```

## Time Management

### CTF Tournament Rules
```
0-5 min:   Triage all challenges, quick-win scan
5-15 min:  Solve all quick-win patterns (base64, ROT13, default creds, etc.)
15-30 min: Tackle medium-difficulty challenges
30-60 min: Work on high-value challenges (200+ points)
60+ min:   Only if very close to solve. Otherwise move on.

CHECKPOINT EVERY 15 MINUTES:
- What challenges are solved?
- What's the easiest unsolved challenge?
- Are we stuck? Move on or buy a hint.
```

### Abandon Criteria (stop working on a challenge)
```
- Stuck for 15 minutes with no new ideas
- No hints purchased yet → buy a hint
- Lower-point challenges remain unsolved
- Challenge requires knowledge we don't have and can't google
- Team energy is low → switch to easier challenge for morale
```

### Solved Challenge Pattern Review
```
EVERY 5 SOLVED CHALLENGES:
1. What patterns are repeating?
2. Can we write a faster script for the next similar challenge?
3. Are we spending too much time on one category?
4. Should we redistribute team effort?
```

## File Organization
```
challenge_name/
├── challenge.*          # Original files
├── solved/              # Extracted/solved files
├── scripts/             # Your solve scripts
│   ├── solve.py
│   └── exploit.py
├── notes.md             # Working notes
└── flag.txt             # Captured flag
```

## Flag Format Recognition

### Common Flag Formats
```
flag{...}          # Most common (lowercase)
CTF{...}           # Common
FLAG{...}          # Sometimes
ctf{...}           # Variant
hitcon{...}        # HITCON CTF
picoCTF{...}       # PicoCTF
HTB{...}           # HackTheBox
THM{...}           # TryHackMe
[1337s-Ur-Flag]    # SpiderCTF format
FS{...}            # FSecure
```

### Flag Validation Checklist
```
Before submitting, verify:
□ Correct flag format for this CTF (check other solved challenges)
□ No trailing/leading whitespace
□ Correct capitalization (flag vs FLAG vs Flag)
□ No extra characters, no URL encoding
□ Flag makes sense contextually (sometimes flags are phrases)
□ Checked for similar flags (flag{ vs flags{ vs flag{typo)
```

## Error Recovery

### Common Failures
```
"Connection refused" → Service is down or port is wrong
"Permission denied" → Need different creds or exploit
"Flag is incorrect" → Wrong flag format, encoding issue, or not the real flag
"No such file" → Challenge files not downloaded correctly
"Syntax error" in script → Debug with -x flag or add print statements
```

### Retry Strategy
```
1. Re-read the challenge description
2. Check if there are hints you missed
3. Look at the solve count — if >100, pattern is probably simple
4. Google "CTF [challenge name] writeup" (you're allowed to research)
5. Ask teammate for fresh eyes
6. If truly stuck, move on. Come back later with fresh perspective.
```

## Team Coordination

### Information Sharing
```
When you find something useful:
1. Claim the challenge: "Working on [challenge_name]"
2. Share discoveries in real-time: "Found LFI at ?page= param"
3. Share scripts: Drop in team's shared directory
4. Flag submission: Only one person submits, announce it
5. Post-mortem: If stuck, describe what you tried
```

### Role Assignment
```
Person A: Web challenges
Person B: Crypto + Forensics
Person C: Pwn + Rev
Person D: Misc + Triage (helps everyone)

Adjust based on team strengths. Rebalance as needed.
```
