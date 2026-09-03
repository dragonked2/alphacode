---
name: tool-doctor
description: Local tool readiness check — Inspect which security tools are installed, available, missing, or degraded. Use when checking tool availability, diagnosing missing tools, or before starting an engagement to verify your toolkit.
---

# TOOL DOCTOR

**Check your tools before you need them. Missing tools mid-engagement = lost time.**

---

## 1. TOOL READINESS CHECK

Run this to see what's available on the current machine:

```bash
#!/bin/bash
echo "╔══════════════════════════════════════════════════════════╗"
echo "║              🔧 TOOL DOCTOR — READINESS CHECK           ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""

# Tool categories
RECON_TOOLS=(subfinder amass assetfinder dnsx massdns)
HTTP_TOOLS=(httpx curl wget)
SCANNER_TOOLS=(nuclei nikto nmap masscan)
FUZZ_TOOLS=(ffuf gobuster dirsearch feroxbuster)
EXPLOIT_TOOLS=(sqlmap dalfox xsstrike commix)
JS_TOOLS=(katana gau waybackurls linkfinder secretfinder)
SECRET_TOOLS=(trufflehog gitleaks)
API_TOOLS=(arjun paramspider kiterunner)
MISC_TOOLS=(checksec searchsploit whatweb wafw00f)

check_tool() {
    local tool=$1
    if command -v "$tool" &>/dev/null; then
        version=$("$tool" --version 2>&1 | head -1)
        echo "  ✅ $tool — $version"
        return 0
    else
        echo "  ❌ $tool — NOT FOUND"
        return 1
    fi
}

echo "📡 RECONNAISSANCE TOOLS"
echo "────────────────────────"
missing=0
for tool in "${RECON_TOOLS[@]}"; do
    check_tool "$tool" || ((missing++))
done
echo ""

echo "🌐 HTTP PROBING TOOLS"
echo "────────────────────────"
for tool in "${HTTP_TOOLS[@]}"; do
    check_tool "$tool" || ((missing++))
done
echo ""

echo "🔍 SCANNER TOOLS"
echo "────────────────────────"
for tool in "${SCANNER_TOOLS[@]}"; do
    check_tool "$tool" || ((missing++))
done
echo ""

echo "📁 FUZZING TOOLS"
echo "────────────────────────"
for tool in "${FUZZ_TOOLS[@]}"; do
    check_tool "$tool" || ((missing++))
done
echo ""

echo "💉 EXPLOITATION TOOLS"
echo "────────────────────────"
for tool in "${EXPLOIT_TOOLS[@]}"; do
    check_tool "$tool" || ((missing++))
done
echo ""

echo "📜 JAVASCRIPT ANALYSIS TOOLS"
echo "────────────────────────"
for tool in "${JS_TOOLS[@]}"; do
    check_tool "$tool" || ((missing++))
done
echo ""

echo "🔑 SECRET SCANNING TOOLS"
echo "────────────────────────"
for tool in "${SECRET_TOOLS[@]}"; do
    check_tool "$tool" || ((missing++))
done
echo ""

echo "🔌 API DISCOVERY TOOLS"
echo "────────────────────────"
for tool in "${API_TOOLS[@]}"; do
    check_tool "$tool" || ((missing++))
done
echo ""

echo "🧰 MISC TOOLS"
echo "────────────────────────"
for tool in "${MISC_TOOLS[@]}"; do
    check_tool "$tool" || ((missing++))
done
echo ""

if [ "$missing" -gt 0 ]; then
    echo "⚠️  $missing tools missing. Install them for full coverage."
    echo ""
    echo "QUICK INSTALL (Debian/Kali/Ubuntu):"
    echo "  apt install nmap sqlmap ffuf gobuster nikto nuclei trivy checksec"
    echo ""
    echo "QUICK INSTALL (macOS):"
    echo "  brew install nmap sqlmap ffuf gobuster nikto nuclei trivy checksec"
    echo ""
    echo "GO TOOLS (from source):"
    echo "  go install github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest"
    echo "  go install github.com/projectdiscovery/httpx/cmd/httpx@latest"
    echo "  go install github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest"
    echo "  go install github.com/projectdiscovery/katana/cmd/katana@latest"
    echo "  go install github.com/projectdiscovery/dnsx/cmd/dnsx@latest"
    echo "  go install github.com/projectdiscovery/naabu/v2/cmd/naabu@latest"
    echo "  go install github.com/hahwul/dalfox/v2@latest"
    echo "  go install github.com/ffuf/ffuf/v2@latest"
else
    echo "✅ All tools ready! You're fully equipped."
fi
```

---

## 2. TOOL STATUS MATRIX

Track tool status for your engagement:

```markdown
## Tool Readiness — example.com Pentest

| Category | Tool | Status | Version | Notes |
|----------|------|--------|---------|-------|
| Recon | subfinder | ✅ Ready | v2.6.3 | |
| Recon | amass | ✅ Ready | v4.2.0 | |
| HTTP | httpx | ✅ Ready | v1.3.7 | |
| HTTP | curl | ✅ Ready | 8.4.0 | |
| Scanner | nuclei | ✅ Ready | v3.1.0 | Templates updated: 2026-01-15 |
| Scanner | nmap | ✅ Ready | 7.94 | |
| Fuzz | ffuf | ✅ Ready | v2.1.0 | |
| Exploit | sqlmap | ✅ Ready | v1.7.12 | |
| JS | katana | ❌ Missing | - | Need for JS crawling |
| Secret | trufflehog | ✅ Ready | v3.60.0 | |

### Missing Tools Impact
- **katana**: Use `gau` + `waybackurls` as alternative for JS crawling
- **dalfox**: Use manual XSS testing with curl
```

---

## 3. TOOL INSTALLATION

### Quick Install by OS

```bash
# ═══════════════════════════════════════════════════════════
# DEBIAN / KALI / UBUNTU
# ═══════════════════════════════════════════════════════════

# Core tools
apt update && apt install -y nmap sqlmap ffuf gobuster nikto

# Go tools
go install github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest
go install github.com/projectdiscovery/httpx/cmd/httpx@latest
go install github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest
go install github.com/projectdiscovery/katana/cmd/katana@latest
go install github.com/projectdiscovery/dnsx/cmd/dnsx@latest
go install github.com/projectdiscovery/naabu/v2/cmd/naabu@latest
go install github.com/hahwul/dalfox/v2@latest
go install github.com/ffuf/ffuf/v2@latest
go install github.com/OJ/gobuster/v3@latest

# Update nuclei templates
nuclei -update-templates

# ═══════════════════════════════════════════════════════════
# macOS
# ═══════════════════════════════════════════════════════════

# Homebrew tools
brew install nmap sqlmap ffuf gobuster nikto

# Go tools (same as Linux)
# ...

# ═══════════════════════════════════════════════════════════
# DOCKER (all platforms)
# ═══════════════════════════════════════════════════════════

# ProjectDiscovery tools
docker pull projectdiscovery/subfinder:latest
docker pull projectdiscovery/httpx:latest
docker pull projectdiscovery/nuclei:latest
docker pull projectdiscovery/katana:latest

# sqlmap
docker pull paoloo/sqlmap
```

---

## 4. TOOL ALTERNATIVES

When a tool is missing, use alternatives:

```
MISSING TOOL     → ALTERNATIVE
═══════════════════════════════════════════════════════════
subfinder        → amass, assetfinder, crt.sh + curl
httpx            → curl -s -o /dev/null -w "%{http_code}"
nuclei           → nikto, manual testing
ffuf             → gobuster, dirsearch, feroxbuster
sqlmap           → manual SQLi testing with curl
dalfox           → manual XSS testing with curl
katana           → gau + waybackurls + manual crawling
dnsx             → dig, nslookup
naabu            → nmap
nmap             → masscan (fast) + manual port check
trufflehog       → gitleaks
arjun            → manual parameter discovery
searchsploit     → exploit-db.com manual search
```

---

## 5. TOOL HEALTH CHECKS

```bash
# Check nuclei templates are up to date
nuclei -update-templates 2>&1 | tail -5

# Check nmap scripts
ls /usr/share/nmap/scripts/ | wc -l

# Check wordlists
ls /usr/share/wordlists/dirb/common.txt 2>/dev/null && echo "✅ wordlists ready" || echo "❌ wordlists missing"

# Check Go path
go version && echo "✅ Go installed" || echo "❌ Go not installed"

# Check Python (for sqlmap, etc.)
python3 --version && echo "✅ Python3 installed" || echo "❌ Python3 not installed"

# Check disk space for evidence collection
df -h . | tail -1 | awk '{print "Disk: " $4 " available"}'
```

---

## 6. QUICK COMMANDS

```bash
# Run tool doctor
/doctor

# Check specific tool
/doctor check nuclei

# Install missing tools
/doctor install

# Show tool alternatives
/doctor alternatives sqlmap
```
