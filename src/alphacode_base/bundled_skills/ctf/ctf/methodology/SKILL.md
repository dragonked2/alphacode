# CTF Methodology — Competition-Grade Playbook

## Core Rule: Ship Flags, Not Reports

Every second spent on documentation is a second not spent on flags.

## Pre-Competition Setup (<5 min)

```bash
mkdir -p ~/ctf/{web,crypto,pwn,rev,forensics,misc,scripts}
pip install pwntools pycryptodome z3-solver angr capstone keystone
apt install steghide binwalk foremost exiftool tshark sqlmap ffuf hydra john hashcat
```

## Automated Challenge Downloading (CTFd)

```bash
CTF_URL="https://ctf.example.com"
TOKEN="YOUR_API_TOKEN"
mkdir -p ~/ctf/challenges && cd ~/ctf/challenges

curl -sH "Authorization: Token $TOKEN" "$CTF_URL/api/v1/challenges" | \
  jq -r '.data[] | "\(.id)|\(.category)|\(.name)|\(.value)|\(.solves)"' | \
  while IFS='|' read -r id cat name val solves; do
    mkdir -p "$cat/$name"
    curl -sH "Authorization: Token $TOKEN" "$CTF_URL/api/v1/challenges/$id/files" | \
      jq -r '.data[] | "\(.url)|\(.name)"' | \
      while IFS='|' read -r url fname; do
        curl -sH "Authorization: Token $TOKEN" -o "$cat/$name/$fname" "$CTF_URL$url"
      done
    echo "$id|$cat|$name|$val|$solves" >> ../challenge_index.txt
  done
```

## Team Coordination & Role Assignment

```
ROLES:
├── Caller       → scoreboard, coordination, hint management
├── Solver-Web   → web + misc only
├── Solver-Crypto→ crypto + rev only
├── Solver-Pwn   → binary exploitation + reversing
├── Solver-Forensics → forensics + OSINT + stego
└── Flex         → jumps to help stuck categories

COMMUNICATION (Discord/Slack):
#ctf-general → status | #ctf-flags → submissions
#ctf-web | #ctf-crypto | #ctf-pwn | #ctf-forensics

STATUS FORMAT (every 5 min):
[SOLVER] [CATEGORY] NAME | Status: SOLVING/STUCK/DONE | Points: X | ETA: Ymin

STUCK PROTOCOL:
1. Declare stuck 2. Flex or Caller intervenes 3. No progress 5 min → abandon
```

## Scoring Strategies

```
DYNAMIC SCORING (CTFd): fewer solves = more points, 0 solves = max value
FIRST BLOOD: +50-150 pts for first solver — race if fast, skip if slow
HINT ECONOMY: -50 pts each, buy at 10-min stuck mark for >150pt challenges

SCORING MANIPULATION:
- Leading → pad score with easy points
- Trailing → chase high-value unsolved
- Last 30 min → hint rush on top unsolved
```

## Time Management

```
DARPA CGC (8hr): 0:05 setup → 0:20 download → 1:00 easy solves → 6:00 systematic → 7:00 hints → 8:00 submit
PicoCTF (12hr): 0:10 setup → 1:00 100pt → 2:00 200pt → 6:00 300-500pt → 10:00 hard → 12:00 hints
HackTheBox (24hr): 0:30 instant → 4:00 web → 8:00 crypto/forensics → 16:00 pwn/rev → 24:00 final

10-MINUTE RULE: no progress → buy hint OR move on. Hint fails 5 min → abandon.
```

## Phase 1: Rapid Triage (first 5 min)

```bash
cd ~/ctf/challenges
grep -rli 'flag{' . && strings */* | grep -iE 'flag\{|ctf\{' && file */*
find . -type f -exec sh -c 'base64 -d "$1" 2>/dev/null | grep -i flag' _ {} \;
```

## Phase 2: Systematic Solving (5-120 min)

```
ORDER: 0-solve → lowest solves → highest points → your specialty
CHECKPOINT EVERY 10 MIN: stuck? → new challenges? → scoreboard?
```

## Phase 3: Endgame (last 30 min)

```
1. BUY ALL hints on top 3 unsolved
2. Share partial solutions
3. Submit anything that looks like a flag
4. Focus on 70%+ solved challenges only
```

## Adaptive Strategy

```
Leading (top 3)   → easy points, pad score
Mid-pack (4-10)   → calculated risks on high-point
Behind (11+)      → ALL hints, ALL quick wins, abandon hard
```

## Knowledge Building

```
EVERY 10 SOLVED: patterns? → write template | weaknesses? → flex there | stuck too long? → 20-min cap
```
