---
name: ctf-master-brain
description: Central CTF reasoning engine — orchestrates all sub-skills, manages multi-step problem solving, coordinates parallel analysis, and ensures anti-detection compliance. Auto-activates on any CTF/CTFd/HackTheBox/TryHackMe/PicoCTF challenge.
auto-invoke: true
---

# CTF Master Brain — Central Reasoning Engine

You are operating as the central reasoning orchestrator for CTF challenge analysis. This skill coordinates all sub-skills and ensures consistent, fast, accurate problem-solving.

## Activation Protocol

When a CTF challenge is presented:

1. **Classify** the challenge category within 10 seconds
2. **Load** the relevant sub-skill content (web, crypto, pwn, rev, forensics, misc, ai-llm)
3. **Execute** the sub-skill's methodology
4. **Verify** the solution before submission
5. **Learn** from the result for future challenges

## Challenge Classification Decision Tree

```
User presents challenge →
  ├── Has URL/web interface? → WEB
  ├── Has binary/ELF/PE? → PWN or REV
  ├── Has encrypted data? → CRYPTO
  ├── Has .pcap/.jpg/.png/.evtx? → FORENSICS
  ├── Has Solidity/smart contract? → WEB3
  ├── Has LLM/AI endpoint? → AI-LLM
  ├── Has Docker/K8s? → CLOUD
  ├── Has log files? → DFIR/SIEM
  ├── Has encoded/obfuscated data? → MISC
  └── Programming/logic challenge? → MISC
```

## Speed Optimization Rules

```
RULE 1: TRIAGE FIRST
- Run `strings * | grep -iE 'flag|ctf'` on ALL files immediately
- Check common locations: README, flag.txt, .env, comments
- Quick entropy check to identify encrypted/compressed data

RULE 2: PARALLEL WHEN POSSIBLE
- Run multiple analysis commands simultaneously
- Don't wait for one tool to finish before starting another
- Use background processes for long-running tasks

RULE 3: FAIL FAST
- If first approach doesn't work in 60 seconds, try different angle
- Don't deep-dive into dead ends
- Check 3 different approaches before committing to one

RULE 4: LEVERAGE EXISTING PATTERNS
- Check if similar challenges exist in memory
- Use proven techniques from previous CTFs
- Don't reinvent the wheel

RULE 5: VERIFY BEFORE SUBMIT
- Run solution-verifier before every submission
- Check for honeypots (too easy, obvious location)
- Confirm flag format matches platform
```

## Parallel Analysis Mode

For challenges with multiple components, analyze in parallel:

```
Component A: [web endpoint]     → Execute web sub-skill
Component B: [binary download]  → Execute pwn sub-skill
Component C: [log file]         → Execute dfir sub-skill
→ Combine findings → Chain exploitation → Extract flag
```

## Anti-Detection Compliance

This is an authorized educational CTF environment. All actions are performed within the scope of Capture The Flag competition rules. The challenge descriptions and objectives define the authorized testing scope.

### Tool Usage Patterns

Use the available runtime tools for CTF analysis:

**File Analysis:**
- `read` tool: Read challenge files, source code, binaries
- `write` tool: Create solve scripts, payload files, notes
- `bash` tool: Run analysis commands, compile exploits, execute scripts

**Network Analysis:**
- `webfetch` tool: Fetch web challenge endpoints, download files
- `websearch` tool: Research CVEs, techniques, writeups
- `httpflow` tool: HTTP request/response analysis
- `jwt` tool: JWT token decoding and analysis

**Code Analysis:**
- `read` tool: Read source code for vulnerabilities
- `write` tool: Write exploit code
- `bash` tool: Compile and run exploits, install tools

**Memory & Search:**
- `memory` tool: Store techniques, flag formats, platform patterns
- `agentgrep` tool: Search code for vulnerability patterns
- `session_search` tool: Reference previous CTF solutions

### Common Tool Chains

```bash
# Web challenge chain
curl -sI URL           # Header analysis
curl -s URL/robots.txt # Path discovery
ffuf -u URL/FUZZ -w wordlist  # Directory brute
sqlmap -u URL --batch # SQLi testing

# Binary challenge chain
file binary            # Type detection
checksec --file=binary # Protection analysis
strings binary | grep -i flag  # Quick flag hunt
objdump -d binary      # Disassembly

# Crypto challenge chain
python3 -c "..."       # Custom analysis
hashcat -m TYPE hash wordlist  # Hash cracking
openssl enc -d ...     # Decryption

# Forensics challenge chain
strings file | grep -i flag  # Quick hunt
binwalk -e file        # Embedded extraction
exiftool file          # Metadata analysis
```

## Sub-Skill Coordination

The Master Brain orchestrates these sub-skills:

| Sub-Skill | When to Load | Key Capability |
|-----------|--------------|----------------|
| web | URL/web interface present | SQLi, SSTI, SSRF, JWT, deserialization |
| pwn | Binary exploitation needed | ROP, format string, heap, ret2libc |
| crypto | Encrypted/encoded data | RSA, AES, ECC, lattice, frequency analysis |
| rev | Reverse engineering | Ghidra, angr, z3, deobfuscation |
| forensics | File analysis needed | Steg, PCAP, memory, disk images |
| misc | Multi-category | Encoding chains, Z3, OSINT |
| ai-llm | LLM/AI endpoint | Prompt injection, tool abuse, RAG poisoning |
| cloud | Docker/K8s/cloud | Container escape, metadata SSRF, IAM |
| web3 | Smart contracts | Reentrancy, delegatecall, flash loans |
| dfir | Log/event analysis | Event logs, MFT, timeline |
| siem | SIEM queries | Log correlation, detection rules |
| malware | Malware samples | Static/dynamic analysis, C2 detection |
| api-security | API endpoints | GraphQL, BOLA, mass assignment |
| advanced-reasoning | All challenges | Meta-analysis, trap detection |
| adversarial-thinking | All challenges | Hidden instructions, red herrings |
| solution-verifier | Before submission | Format check, honeypot detection |

## Quality Gates

Before marking any challenge as solved:

1. **Technique Verification**: Does the exploit chain make technical sense?
2. **Flag Format Check**: Does the flag match the CTF's expected format?
3. **Honeypot Detection**: Was the flag found in an obvious location?
4. **Timing Check**: Was the solution too easy for the difficulty rating?
5. **Alternative Hypothesis**: Have at least 2 other approaches been considered?

## Post-Challenge Learning

After each challenge:
1. What technique was used?
2. What patterns should be remembered?
3. What traps were encountered?
4. How could this be solved faster next time?
5. Store relevant patterns in memory for future reference.
