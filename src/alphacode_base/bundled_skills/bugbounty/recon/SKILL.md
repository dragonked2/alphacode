---
name: recon
description: Reconnaissance and OSINT — Subdomain enumeration, attack surface mapping, technology detection. Use when starting a new engagement, when user mentions recon, or when mapping an attack surface. Includes passive and active reconnaissance techniques.
---

# 🎯 Reconnaissance Skill

Elite-level reconnaissance and attack surface mapping.

## Reconnaissance Checklist

### Passive Recon
- [ ] Subdomain enumeration (crt.sh, SecurityTrails, VirusTotal)
- [ ] DNS records (A, AAAA, MX, NS, TXT, CNAME)
- [ ] Certificate transparency logs
- [ ] Wayback Machine URLs
- [ ] GitHub/GitLab code leaks
- [ ] Shodan/Censys host information

### Active Recon
- [ ] HTTP probing and technology detection
- [ ] Port scanning
- [ ] Directory fuzzing
- [ ] JavaScript analysis
- [ ] API endpoint discovery

### OSINT
- [ ] Employee enumeration (LinkedIn, GitHub)
- [ ] Email harvesting
- [ ] Password leak checking
- [ ] Social media analysis

## Tools & Commands

### Subdomain Enumeration
```bash
# crt.sh (Certificate Transparency)
curl -s "https://crt.sh/?q=%.target.com&output=json" | jq -r '.[].name_value' | sort -u

# subfinder
subfinder -d target.com -all -o subs.txt

# amass (passive)
amass enum -passive -d target.com -o amass_passive.txt

# amass (active)
amass enum -active -d target.com -brute -w /usr/share/wordlists/subdomains.txt -o amass_active.txt

# assetfinder
assetfinder --subs-only target.com > assetfinder.txt

# Combine all
cat subs.txt amass_passive.txt amass_active.txt assetfinder.txt | sort -u > all_subs.txt
```

### DNS Resolution
```bash
# dnsx
dnsx -l all_subs.txt -a -aaaa -cname -mx -ns -txt -resp -o resolved.txt

# massdns
massdns -r /usr/share/wordlists/resolvers.txt -t A -o S all_subs.txt -w resolved_massdns.txt
```

### HTTP Probing
```bash
# httpx
httpx -l resolved.txt -sc -title -tech-detect -cdn -o alive.txt

# httpx with more options
httpx -l resolved.txt -sc -title -tech-detect -cdn -follow-redirects -o alive_detailed.txt
```

### Port Scanning
```bash
# nmap
nmap -sV -sC -T4 -p- -oA nmap_full target.com

# masscan
masscan target.com -p0-65535 --rate=1000 -oG masscan.txt
```

### Directory Fuzzing
```bash
# ffuf
ffuf -u https://target.com/FUZZ -w /usr/share/wordlists/dirb/common.txt -o fuzz.json

# dirsearch
dirsearch -u https://target.com -w /usr/share/wordlists/dirb/common.txt -o dirsearch.txt

# gobuster
gobuster dir -u https://target.com -w /usr/share/wordlists/dirb/common.txt -o gobuster.txt
```

### JavaScript Analysis
```bash
# katana (crawler)
katana -u target.com -d 3 -jc -o urls_katana.txt

# gau (Get All URLs)
gau target.com | grep -vE "\.(css|js|png|jpg|gif)" > urls_gau.txt

# waybackurls
waybackurls target.com > wayback.txt

# LinkFinder
python3 LinkFinder.py -i https://target.com -d -o results.html

# Secret Finder
python3 SecretFinder.py -i https://target.com/app.js -o secret_results.html
```

### Google Dorks
```
site:target.com
site:target.com filetype:pdf
site:target.com filetype:sql
site:target.com filetype:env
site:target.com filetype:log
site:target.com intitle:"index of"
site:target.com inurl:admin
site:target.com inurl:login
site:target.com inurl:api
site:target.com ext:sql | ext:bak | ext:old
site:target.com "password" | "credentials" | "secret"
```

### Shodan/Censys
```bash
# Shodan
shodan search hostname:target.com --fields ip_str,port,product

# Censys
censys search "services.tls.certificates.leaf_data.names: target.com"
```

## Recon Workflow

1. **Passive recon first** — crt.sh, SecurityTrails, GitHub
2. **Subdomain enumeration** — combine multiple tools
3. **DNS resolution** — find live hosts
4. **HTTP probing** — identify web servers
5. **Technology detection** — identify frameworks, CMS
6. **Directory fuzzing** — find hidden paths
7. **JavaScript analysis** — find secrets, endpoints
8. **API discovery** — map all endpoints

## Output Structure
```
recon/
├── subs.txt          # All subdomains
├── resolved.txt      # DNS resolution
├── alive.txt         # Live HTTP hosts
├── nmap/             # Port scan results
├── fuzz/             # Directory fuzzing
├── urls/             # Discovered URLs
├── js/               # JavaScript analysis
└── report.md         # Recon summary
```

## Automation Script
```bash
#!/bin/bash
TARGET=$1
mkdir -p recon && cd recon

echo "[*] Passive subdomain enumeration..."
curl -s "https://crt.sh/?q=%.$TARGET&output=json" | jq -r '.[].name_value' | sort -u > crtsh.txt
subfinder -d $TARGET -all -o subfinder.txt

echo "[*] DNS resolution..."
cat crtsh.txt subfinder.txt | sort -u | dnsx -a -aaaa -cname -mx -ns -txt -resp -o resolved.txt

echo "[*] HTTP probing..."
httpx -l resolved.txt -sc -title -tech-detect -cdn -o alive.txt

echo "[*] Port scanning..."
nmap -sV -sC -T4 -p- -oA nmap_full $TARGET

echo "[*] Directory fuzzing..."
ffuf -u https://$TARGET/FUZZ -w /usr/share/wordlists/dirb/common.txt -o fuzz.json

echo "[*] JavaScript analysis..."
katana -u $TARGET -d 3 -jc -o urls.txt

echo "[+] Recon complete! Check recon/ directory."
```
