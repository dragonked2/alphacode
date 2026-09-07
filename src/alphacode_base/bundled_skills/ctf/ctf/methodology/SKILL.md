---
name: ctf-methodology
description: "CTF workflow methodology skill. When user mentions CTF strategy, challenge triage, flag format detection, time management, team coordination, write-up templates, or CTF planning. Covers the complete CTF methodology including challenge triage rules, flag format detection patterns, time management strategies, team coordination protocols, and write-up templates for all challenge categories."
---

# CTF Methodology — Competition Strategy Brain

This skill covers the strategic layer of CTF competitions:
1. **Triage systematically** — classify and prioritize every challenge
2. **Detect flags early** — recognize format patterns instantly
3. **Manage time ruthlessly** — abandon dead ends, pivot fast
4. **Coordinate as a team** — divide and conquer
5. **Document everything** — write-ups earn bonus points

---

## Challenge Triage

### The 10-Minute Rule
```
RULE: Spend MAX 10 minutes on any challenge before moving on.

MINUTE 0-2: Read description, check provided files
MINUTE 2-4: Identify category and difficulty
MINUTE 4-6: Attempt initial approach
MINUTE 6-8: If stuck, try alternative approach
MINUTE 8-10: Document progress and move on

AFTER 10 MINUTES:
□ Is there a clear path forward? → Continue
□ Are you stuck with no new ideas? → Move on
□ Is a teammate making progress? → Let them handle it
□ Is the point value worth more time? → Maybe continue
```

### Triage Process
```
STEP 1: LIST ALL CHALLENGES
Challenge Name | Category | Points | Status
_______________|__________|________|_______
```

### Difficulty Assessment
```
EASY (solve in <15 min):
□ Single-step encoding
□ Simple tool usage
□ Obvious vulnerability
□ Low point value (<100)

MEDIUM (solve in 15-45 min):
□ Multi-step process
□ Requires specific knowledge
□ Moderate tool usage
□ Medium point value (100-300)

HARD (solve in 45-120 min):
□ Complex chain required
□ Custom exploitation
□ Advanced tooling
□ High point value (300-500)

INSANE (solve in 2+ hours):
□ Novel technique required
□ Extensive research needed
□ Multi-category chain
□ Very high point value (500+)
```

---

## Flag Format Detection

### Common Flag Formats
```python
import re

FLAG_PATTERNS = [
    # Standard formats
    r'flag\{[^\}]+\}',
    r'CTF\{[^\}]+\}',
    r'FLAG\{[^\}]+\}',
    r'HTB\{[^\}]+\}',
    r'hctf\{[^\}]+\}',
    r'actf\{[^\}]+\}',
    r'cta\{[^\}]+\}',
    r'picoCTF\{[^\}]+\}',
    r'DUCTF\{[^\}]+\}',
    r'bugctf\{[^\}]+\}',
    r'wctf\{[^\}]+\}',
    r'0ctf\{[^\}]+\}',
    r'midnight\{[^\}]+\}',
    r'SECCTF\{[^\}]+\}',
    
    # Hash formats (raw)
    r'[a-f0-9]{32}',  # MD5
    r'[a-f0-9]{40}',  # SHA1
    r'[a-f0-9]{64}',  # SHA256
    
    # Base64 encoded flags
    r'ZmxhZ3s[^\}]+\}',  # "flag{" in base64
    
    # Custom formats (from challenge description)
    # Always check challenge description for flag format
]

def find_flags(text):
    flags = []
    for pattern in FLAG_PATTERNS:
        matches = re.findall(pattern, text, re.IGNORECASE)
        flags.extend(matches)
    return flags
```

### Flag Validation
```python
def validate_flag(flag, expected_format=None):
    # Remove whitespace
    flag = flag.strip()
    
    # Check if it looks like a flag
    if not flag:
        return False
    
    # Check common formats
    if re.match(r'flag\{[^\}]+\}', flag, re.IGNORECASE):
        return True
    if re.match(r'CTF\{[^\}]+\}', flag, re.IGNORECASE):
        return True
    if re.match(r'HTB\{[^\}]+\}', flag, re.IGNORECASE):
        return True
    
    # Check if it's a hash
    if re.match(r'^[a-f0-9]{32,64}$', flag, re.IGNORECASE):
        return True
    
    # Check if it's printable and reasonable length
    if flag.isprintable() and 10 < len(flag) < 200:
        return True
    
    return False
```

### Flag Extraction from Files
```bash
# From any file
strings file | grep -iE "flag|ctf|key|secret|password"
strings file | grep -E "\{[^\}]+\}"

# From binary
strings binary | grep -i flag

# From images
exiftool image.png | grep -i flag
zsteg image.png | grep -i flag

# From pcap
tshark -r capture.pcap -Y "http contains flag"
```

---

## Time Management

### Competition Timeline
```
4-HOUR COMPETITION:
0:00 - 0:05  → Recon (list all challenges)
0:05 - 0:15  → Triage (attempt all challenges briefly)
0:15 - 1:00  → Solve easy challenges
1:00 - 2:00  → Solve medium challenges
2:00 - 3:30  → Solve hard challenges
3:30 - 4:00  → Review and submit remaining flags

8-HOUR COMPETITION:
0:00 - 0:10  → Recon
0:10 - 0:30  → Triage
0:30 - 2:00  → Easy challenges
2:00 - 4:00  → Medium challenges
4:00 - 6:00  → Hard challenges
6:00 - 7:30  → Remaining challenges
7:30 - 8:00  → Review and submit

24-HOUR COMPETITION:
Day 1 Morning   → Recon, Triage, Easy
Day 1 Afternoon → Medium
Day 1 Evening   → Hard
Day 2 Morning   → Remaining hard
Day 2 Afternoon → Review and submit
```

### Rotation Schedule
```
EVERY 30 MINUTES:
□ Am I making progress?
□ Should I switch challenges?
□ Have I submitted all found flags?
□ Are teammates stuck?
□ Is the scoreboard changing?

RED FLAGS (switch immediately):
□ No progress in 20 minutes
□ Wrong approach identified
□ Missing required tools
□ Challenge is way above your skill level
□ Teammate is ahead on this challenge
```

### Abandonment Criteria
```
ABANDON WHEN:
□ Stuck for > 20 minutes
□ Required tool unavailable
□ Challenge requires unknown knowledge
□ Point value too low for time invested
□ Teammate solving it
□ Better opportunities elsewhere

NEVER ABANDON WHEN:
□ You've found a working approach
□ You're close to the flag
□ It's a unique category no one else can solve
□ It's worth significant points
```

---

## Team Coordination

### Role Assignments (Team of 4)
```
ROLE 1: Recon Lead
  Responsibilities:
  - Map all challenges
  - Track scoreboard
  - Monitor competition announcements
  - Coordinate team efforts

ROLE 2: Web/Crypto Expert
  Responsibilities:
  - Solve web challenges
  - Solve crypto challenges
  - Share techniques with team

ROLE 3: Pwn/Rev Expert
  Responsibilities:
  - Solve binary challenges
  - Solve reverse engineering
  - Share techniques with team

ROLE 4: Forensics/Misc Expert
  Responsibilities:
  - Solve forensics challenges
  - Solve miscellaneous
  - Document solutions
```

### Communication Protocol
```
CHANNEL STRUCTURE:
#general     → Strategy, scoreboard, announcements
#web         → Web challenge discussion
#crypto      → Crypto challenge discussion
#pwn         → Binary exploitation discussion
#rev         → Reverse engineering discussion
#forensics   → Forensics discussion
#misc        → Miscellaneous discussion
#flags       → Flag submissions

MESSAGING FORMAT:
[CHALLENGE] [STATUS] [MESSAGE]

Example:
[login-page] [IN-PROGRESS] Found SQL injection, testing payloads
[login-page] [SOLVED] Flag: flag{...}
[crypto-easy] [STUCK] Tried RSA, no progress, help needed
```

### Handoff Protocol
```
WHEN STUCK:
1. Post: "Stuck on [challenge] at [step]"
2. Document what you've tried
3. List what you think the next step is
4. Another member picks up or suggests approach

WHEN SOLVING:
1. Post: "Found [vulnerability/technique] on [challenge]"
2. Share your approach
3. Document the solution method
4. Submit flag and update scoreboard

WHEN DONE:
1. Post: "Solved [challenge], flag: [flag]"
2. Write up the solution
3. Help others with similar challenges
```

---

## Write-Up Templates

### Web Challenge Write-Up
```markdown
# [Challenge Name] — Web ([Points])

## Challenge
[Description of the challenge]

## Reconnaissance
[What you found during initial survey]

## Vulnerability
[Description of the vulnerability found]

## Exploitation
### Step 1: [Initial Access]
[How you started the attack]

### Step 2: [Privilege Escalation]
[How you escalated]

### Step 3: [Flag Extraction]
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

### Crypto Challenge Write-Up
```markdown
# [Challenge Name] — Crypto ([Points])

## Challenge
[Description of the challenge]

## Analysis
[Analysis of the cryptographic scheme]

## Attack
### Step 1: [Identify Weakness]
[What weakness was found]

### Step 2: [Implement Attack]
[How you implemented the attack]

### Step 3: [Extract Flag]
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

### Pwn Challenge Write-Up
```markdown
# [Challenge Name] — Pwn ([Points])

## Challenge
[Description of the challenge]

## Binary Analysis
[Analysis of the binary, protections, vulnerabilities]

## Exploitation
### Step 1: [Find Vulnerability]
[What vulnerability was found]

### Step 2: [Develop Exploit]
[How you developed the exploit]

### Step 3: [Get Shell/Flag]
[How you got code execution or the flag]

## Flag
`flag{...}`

## Exploit Code
```python
[Full exploit code]
```

## Tools Used
- [Tool 1]
- [Tool 2]

## Time Taken
[X minutes]

## Key Takeaway
[What to remember for similar challenges]
```

### Rev Challenge Write-Up
```markdown
# [Challenge Name] — Rev ([Points])

## Challenge
[Description of the challenge]

## Analysis
[Analysis of the binary, what it does]

## Reversing
### Step 1: [Understand Logic]
[What the binary does]

### Step 2: [Find Key Logic]
[Key algorithm or check]

### Step 3: [Extract Flag]
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

### Forensics Challenge Write-Up
```markdown
# [Challenge Name] — Forensics ([Points])

## Challenge
[Description of the challenge]

## Analysis
[Analysis of the provided files]

## Findings
### Step 1: [Initial Survey]
[What you found first]

### Step 2: [Deep Analysis]
[What you discovered]

### Step 3: [Flag Extraction]
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

## Competition Checklist

### Before Competition
```
□ Tools installed and working
□ Wordlists downloaded
□ Scripts ready (pwntools, etc.)
□ Team communication set up
□ Notes template ready
□ Flag submission system ready
□ Backup tools available
□ Snacks and drinks ready
```

### During Competition
```
□ Recon complete
□ All challenges triaged
□ Easy challenges solved
□ Medium challenges attempted
□ Hard challenges attempted
□ Flags submitted
□ Write-ups started
□ Team coordination active
```

### After Competition
```
□ All flags submitted
□ Write-ups completed
□ Lessons learned documented
□ Tools updated
□ Skills gaps identified
□ Practice plan created
```

---

## Quick Reference: Decision Matrix

```
DECISION: Which challenge to solve next?
→ Lowest expected time / highest points

DECISION: Keep trying or move on?
→ If no progress in 10 min, move on

DECISION: Solo or team effort?
→ Solo if you're close, team if stuck

DECISION: Easy or hard challenge?
→ Easy first for quick points

DECISION: Document now or later?
→ Document immediately after solving
```

---

**Remember:** CTF is a team sport. Communication, coordination, and time management are as important as technical skills. Always triage systematically, detect flags early, and write up solutions for the team.
