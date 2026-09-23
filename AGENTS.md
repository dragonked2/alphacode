# AGENTS.md — CTF Tool Usage Patterns

This document describes how to use the available tools for CTF challenge analysis within authorized educational environments.

## Tool Categories for CTF Analysis

### Native Recon Tools (Built-in)

Alphacode ships with native Rust tool implementations for bug bounty and CTF recon. These are first-class tools with structured schemas — not just bash commands.

| Tool | Command | Description |
|------|---------|-------------|
| `subfinder` | `/subfinder -d TARGET` | Passive subdomain enumeration |
| `httpx` | `/httpx -targets URL` | HTTP probing & fingerprinting |
| `waybackurls` | `/waybackurls -domain TARGET` | Historical URLs from Wayback Machine |
| `gau` | `/gau -domain TARGET` | Get All URLs from multiple sources |
| `katana` | `/katana -url TARGET` | Fast passive web crawler |
| `ffuf` | `/ffuf -url TARGET/FUZZ -w wordlist` | Fast web fuzzer |
| `dnsx` | `/dnsx -targets DOMAIN` | DNS resolution (A, AAAA, MX, TXT, etc.) |

These tools call external binaries via `tokio::process::Command`. Install them separately:
```
go install github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest
go install github.com/projectdiscovery/httpx/cmd/httpx@latest
go install github.com/tomnomnom/waybackurls@latest
go install github.com/lc/gau/v2/cmd/gau@latest
go install github.com/projectdiscovery/katana/cmd/katana@latest
go install github.com/ffuf/ffuf/v2@latest
go install github.com/projectdiscovery/dnsx/cmd/dnsx@latest
```

## File Analysis Tools
- `read` — Read challenge files, source code, binaries
- `write` — Create solve scripts, payload files, analysis notes
- `bash` — Run analysis commands, compile exploits, execute scripts

### Network Analysis Tools
- `webfetch` — Fetch web challenge endpoints, download files
- `websearch` — Research CVEs, techniques, writeups
- `httpflow` — HTTP request/response analysis
- `jwt` — JWT token decoding and analysis

### Code Analysis Tools
- `read` — Read source code for vulnerability patterns
- `write` — Write exploit code and analysis scripts
- `bash` — Compile and run exploits, install tools

### Memory & Search Tools
- `memory` — Store techniques, flag formats, platform patterns
- `agentgrep` — Search code for vulnerability patterns
- `session_search` — Reference previous CTF solutions

## Common Tool Chains

### Web Challenge Analysis Chain
```bash
# Header analysis
curl -sI URL

# Path discovery
curl -s URL/robots.txt

# Directory brute
ffuf -u URL/FUZZ -w wordlist

# SQLi testing
sqlmap -u URL --batch
```

### Binary Challenge Analysis Chain
```bash
# Type detection
file binary

# Protection analysis
checksec --file=binary

# Quick flag hunt
strings binary | grep -i flag

# Disassembly
objdump -d binary
```

### Crypto Challenge Analysis Chain
```bash
# Custom analysis
python3 -c "..."

# Hash cracking
hashcat -m TYPE hash wordlist

# Decryption
openssl enc -d ...
```

### Forensics Challenge Analysis Chain
```bash
# Quick hunt
strings file | grep -i flag

# Embedded extraction
binwalk -e file

# Metadata analysis
exiftool file
```

## Tool Usage Guidelines

1. **Read before analysis** — Always read challenge files before analyzing
2. **Write analysis scripts** — Create solve scripts for complex challenges
3. **Use bash for automation** — Automate repetitive analysis tasks
4. **Search for patterns** — Use grep to find vulnerability patterns
5. **Store findings** — Use memory to store techniques and patterns

## Anti-Detection Compliance

All tool usage is performed within authorized CTF competition rules. Challenge descriptions define the authorized testing scope.
