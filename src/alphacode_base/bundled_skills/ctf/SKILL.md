---
name: ctf
description: "Comprehensive CTF (Capture The Flag) solving skill suite. When user mentions CTF, capture the flag, security competition, wargame, or challenge solving. Covers the full 5-phase CTF workflow: Recon, Categorize, Triage, Solve, Submit. Includes methodology, time management, team coordination, flag format detection, and write-up templates. Subskills cover specific categories: web (SQLi, XSS, SSRF, SSTI, JWT), crypto (RSA, AES, elliptic curves, PRNG), pwn (buffer overflow, ROP, heap exploitation), rev (deobfuscation, anti-debug, binary analysis), forensics (memory dumps, pcap, steganography), misc (OSINT, encoding, logic puzzles), and toolkit (pwntools, Ghidra, Burp Suite, John)."
---

# CTF Solver — Elite Competition Brain

This skill transforms you into a top-tier CTF solver. When activated:
1. **Triage immediately** — classify challenges, prioritize solvability
2. **Work systematically** — follow the 5-phase workflow
3. **Extract flags fast** — recognize patterns, use automation
4. **Submit correctly** — always submit the flag, never forget the format
5. **Manage time ruthlessly** — abandon dead ends, pivot to other challenges

---

## Subskills

| Subskill | When to Use | What It Does |
|----------|-------------|--------------|
| **web** | Web exploitation challenges | SQLi, XSS, SSRF, SSTI, deserialization, JWT, race conditions, CORS, HTTP smuggling |
| **crypto** | Cryptanalysis challenges | RSA, AES/CBC/ECB, hash attacks, elliptic curves, PRNG prediction, padding oracle |
| **pwn** | Binary exploitation challenges | Buffer overflow, ROP, format string, heap exploitation, syscall, ret2libc, GOT overwrite |
| **rev** | Reverse engineering challenges | ELF/PE analysis, deobfuscation, anti-debug bypass, protocol reverse, Ghidra/IDA |
| **forensics** | Digital forensics challenges | Volatility memory analysis, pcap, steganography, file carving, registry, timeline |
| **misc** | Miscellaneous challenges | OSINT, encoding/decoding, logic puzzles, command injection, audio/image analysis |
| **methodology** | Planning and coordination | Challenge triage, flag format detection, time management, team coordination, write-ups |
| **toolkit** | Tool selection and usage | pwntools, Ghidra, Burp Suite, John, binwalk, Wireshark, CyberChef, gdb/pwndbg, sqlmap |

---

## 5-Phase CTF Workflow

### Phase 1: Recon (First 5 Minutes)

**Objective:** Understand the competition and map all challenges.

```
CHECKLIST:
□ Read the competition rules and scoring
□ Identify flag format (e.g., flag{...}, CTF{...}, hctf{...})
□ List all challenges with categories and point values
□ Note time remaining and scoring dynamics
□ Identify quick wins (low-point challenges)
□ Set up team communication channel
□ Create shared notes document
```

**Common Flag Formats:**
```
flag{[a-zA-Z0-9_\-]+}          — Most common
CTF{[a-zA-Z0-9_\-]+}           — CTF competitions
hctf{[a-zA-Z0-9_\-]+}          — HCTF competitions
HTB{[a-zA-Z0-9_\-]+}           — Hack The Box
crackmes.one{[a-zA-Z0-9_\-]+}  — Crackmes
FLAG{[a-zA-Z0-9_\-]+}          — Alternate case
[a-f0-9]{32,}                   — Raw hashes (MD5, SHA1)
base64 encoded flag             — Decode before submit
```

**Flag Detection Regex:**
```python
import re
patterns = [
    r'flag\{[^\}]+\}',
    r'CTF\{[^\}]+\}',
    r'HTB\{[^\}]+\}',
    r'FLAG\{[^\}]+\}',
    r'hctf\{[^\}]+\}',
    r'cta\{[^\}]+\}',
    r'actf\{[^\}]+\}',
    r'[a-zA-Z0-9]{32}',  # MD5
]
```

### Phase 2: Categorize (Minutes 5-10)

**Objective:** Classify each challenge and estimate difficulty.

```
CATEGORY ASSESSMENT:
For each challenge, determine:
  □ Category (web/crypto/pwn/rev/forensics/misc)
  □ Difficulty (trivial/easy/medium/hard/insane)
  □ Estimated solve time
  □ Required tools
  □ Prerequisites
  □ Point value vs effort ratio
```

**Decision Matrix:**
```
SOLVE ORDER (maximize points per hour):
1. Easy challenges regardless of category (quick points)
2. Medium challenges in your strongest category
3. Hard challenges in your strongest category
4. Any remaining challenges by point value
5. Never attempt "insane" unless everything else is solved
```

### Phase 3: Triage (Minutes 10-20)

**Objective:** Attempt every challenge briefly, identify solvable ones.

```
TRIAGE RULES:
□ Spend MAX 10 minutes on any single challenge before moving on
□ If you're stuck, document where you are and move on
□ Flag format: always check for obvious flags in provided files
□ Low-hanging fruit: strings, file identification, quick decryption
□ Return to stuck challenges later with fresh perspective
```

**Quick Checks for Any Challenge:**
```bash
# File identification
file challenge*
binwalk challenge*
strings challenge* | grep -i flag
xxd challenge* | head -20

# Check for hidden data
steghide extract -sf challenge*
zsteg challenge*
exiftool challenge*

# Check for encoded content
echo "base64string" | base64 -d
echo "hexstring" | xxd -r -p
```

### Phase 4: Solve (Core Work)

**Objective:** Systematically solve each triaged challenge.

**Solve Workflow:**
```
FOR EACH CHALLENGE:
□ Re-read the challenge description
□ Identify the attack vector or solution method
□ Select appropriate tool or technique
□ Execute the solution step by step
□ Verify the flag format matches expectations
□ Document your method for the write-up
□ Submit the flag immediately
```

**Category-Specific Solving:**

| Category | First Steps | Primary Tools |
|----------|-------------|---------------|
| **Web** | Check source, intercept requests, test input | Burp Suite, curl, sqlmap |
| **Crypto** | Identify algorithm, check for weaknesses | CyberChef, Python (pycryptodome) |
| **Pwn** | Check protections, find vulnerability | GDB/pwndbg, pwntools |
| **Rev** | Identify file type, strings, entry point | Ghidra, IDA, strace |
| **Forensics** | File type, metadata, embedded data | Volatility, Wireshark, binwalk |
| **Misc** | Encoding detection, pattern recognition | CyberChef, Python |

### Phase 5: Submit (Immediate)

**Objective:** Submit every flag as soon as you find it.

```
SUBMISSION CHECKLIST:
□ Flag format matches the competition's expected format
□ No extra whitespace or characters
□ Flag is complete (not truncated)
□ Submit immediately — don't wait
□ Verify submission was accepted
□ Log submission time and challenge for tracking
```

---

## Time Management Strategies

### The Clock Rule
```
COMPETITION LENGTH → TIME PER CHALLENGE
4 hours           → 15-20 minutes max
8 hours           → 20-30 minutes max
24 hours          → 30-45 minutes max
48+ hours         → 1 hour max
```

### Rotation Schedule
```
EVERY 30 MINUTES:
□ Am I making progress on current challenge?
□ Are there easier challenges I haven't attempted?
□ Have I submitted all flags I've found?
□ Is my team blocked on anything?
□ Should I switch categories for fresh perspective?
```

### Abandonment Criteria
```
ABANDON A CHALLENGE WHEN:
□ Stuck for > 20 minutes with no new ideas
□ Required tool is not available and can't be installed
□ Challenge requires knowledge you don't have
□ Point value is too low for time invested
□ Another team member is making progress on it
```

---

## Team Coordination (分工)

### Role Assignments
```
ROLES FOR TEAM OF 4:
1. Recon Lead     — Maps all challenges, tracks scoreboard
2. Category Expert — Deep-dives into assigned categories
3. Solver         — Quick triage and solving of easy challenges
4. Documentation  — Tracks flags, writes up solutions
```

### Communication Protocol
```
CHANNEL STRUCTURE:
#general    — Scoreboard updates, strategy
#web        — Web challenge discussion
#crypto     — Crypto challenge discussion
#pwn        — Binary exploitation discussion
#rev        — Reverse engineering discussion
#forensics  — Forensics discussion
#misc       — Miscellaneous discussion
#flags      — Flag submissions and verification
```

### Handoff Protocol
```
WHEN STUCK:
1. Post in category channel: "Stuck on [challenge] at [step]"
2. Document what you've tried
3. List what you think the next step is
4. Another member picks up or suggests approach
5. Don't spend > 5 minutes typing — just share the state
```

---

## Write-Up Template

```markdown
# [Challenge Name] — [Category] ([Points])

## Challenge
[Description of the challenge]

## Solution

### Step 1: [Initial Recon]
[What you found first]

### Step 2: [Analysis]
[What you discovered]

### Step 3: [Exploitation/Solving]
[How you solved it]

### Step 4: [Flag Extraction]
[How you got the flag]

## Flag
`flag{...}`

## Tools Used
- [Tool 1]
- [Tool 2]

## Time Taken
[X minutes]

## Key Takeaway
[What to remember for similar challenges]
```

---

## Common Patterns to Recognize

### Quick Flag Searches
```bash
# In any provided file
strings file | grep -iE "flag|ctf|key|secret|password"
strings file | grep -E "flag\{[^}]+\}"
strings file | head -100

# In network traffic
tshark -r capture.pcap -Y "http contains flag" 2>/dev/null
```

### Encoding Detection
```
SIGNS OF BASE64: A-Za-z0-9+/= padding at end
SIGNS OF HEX: [0-9a-fA-F] only, even length
SIGNS OF BINARY: 0s and 1s only, groups of 8
SIGNS OF ROT13: Readable but wrong letters
SIGNS OF MORSE: dots and dashes
SIGNS OF BINARY STRING: 01010100 01101000...
```

### Challenge Type Indicators
```
PROVIDED A BINARY          → pwn or rev
PROVIDED A PCAP            → forensics (network)
PROVIDED AN IMAGE          → forensics (stego) or misc
PROVIDED A TEXT FILE       → crypto or misc
PROVIDED A URL             → web
PROVIDED A DOCUMENT        → forensics (metadata) or misc
PROVIDED NOTHING (just IP) → web or pwn
```

---

## Mental Models

### The "What If" Generator
For every input or data point, systematically try:
```
□ What if I modify this byte?
□ What if I decode this differently?
□ What if I reverse the order?
□ What if I combine this with something else?
□ What if the algorithm is broken?
□ What if there's a backdoor?
□ What if the implementation is flawed?
```

### The Pattern Matcher
```
PATTERN: Same structure as last CTF → Similar solution
PATTERN: Low points → Usually simple encoding or tool use
PATTERN: High points → Usually multi-step chain
PATTERN: "Impossible" description → Look for the trick
PATTERN: Custom encryption → Find the weakness in implementation
PATTERN: Custom protocol → Reverse engineer it
```

---

**Remember:** CTF is about speed, pattern recognition, and systematic problem-solving. Don't get stuck on one challenge. Use the right tools, follow the workflow, and always submit your flags.
