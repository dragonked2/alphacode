# CTF Toolkit Skill

## Speed-First Approach: Use the right tool instantly

### Phase 1: Tool Selection (<30 seconds)
```
MATCH challenge to tool:
├── Binary exploitation → pwntools, ROPgadget, ropper, gdb, checksec
├── Web exploitation → curl, ffuf, sqlmap, nikto, hydra
├── Cryptography → python3, hashcat, john, openssl, sage
├── Reverse engineering → Ghidra, IDA, radare2, angr, z3
├── Forensics → tshark, binwalk, exiftool, steghide, foremost
├── DFIR → Volatility 3, EvtxECmd, MFTECmd, PECmd, Timeline Explorer
├── Malware → PE Studio, DIE, capa, FLOSS, olevba, YARA
├── SIEM → Splunk queries, ELK/Kibana, deepbluecli
├── Misc → CyberChef, zbarimg, tesseract, multimon-ng
└── General → strings, xxd, file, grep, python3
```

### Phase 2: Tool Execution (<5 minutes)
```bash
# Run tool with standard options
# Adjust based on challenge requirements
```

### Phase 3: Analyze and Submit
```
IF flag found → submit immediately
ELSE → try different tool (come back later if stuck)
```

## Essential Tool Commands

### pwntools (Binary Exploitation)
```python
from pwn import *

# Remote connection
p = remote('challenge.ctf.com', 1337)

# Local process
p = process('./binary')

# ELF analysis
elf = ELF('./binary')
print(hex(elf.symbols['main']))
print(hex(elf.got['puts']))
print(hex(elf.plt['puts']))

# ROP
rop = ROP(elf)
print(rop.dump())

# GDB
gdb.attach(p, 'b main')

# Interaction
p.sendline(b'payload')
p.recvuntil(b'> ')
print(p.recvline())

# Pack/Unpack
p64(0x401186)  # Pack 64-bit address
u64(b'A' * 8)  # Unpack 64-bit address
```

### curl (Web Exploitation)
```bash
# Basic request
curl -v http://target.com

# POST request
curl -X POST http://target.com/login -d "username=admin&password=pass"

# Headers
curl -H "Authorization: Bearer token" http://target.com/api

# Cookies
curl -b cookies.txt http://target.com
curl -c cookies.txt http://target.com/login

# Follow redirects
curl -L http://target.com

# Save output
curl -o output.txt http://target.com/secret

# Proxy
curl -x http://proxy:8080 http://target.com
```

### ffuf (Web Fuzzing)
```bash
# Directory fuzzing
ffuf -u http://target.com/FUZZ -w /usr/share/seclists/Discovery/Web-Content/common.txt

# Parameter fuzzing
ffuf -u "http://target.com/?FUZZ=test" -w /usr/share/seclists/Discovery/Web-Content/burp-parameter-names.txt

# POST fuzzing
ffuf -u http://target.com/login -X POST -d "username=admin&password=FUZZ" -w passwords.txt

# Header fuzzing
ffuf -u http://target.com -H "FUZZ: value" -w headers.txt

# Filter by response size
ffuf -u http://target.com/FUZZ -w wordlist.txt -fs 4242

# Filter by status code
ffuf -u http://target.com/FUZZ -w wordlist.txt -fc 404
```

### sqlmap (SQL Injection)
```bash
# Basic injection
sqlmap -u "http://target.com/?id=1" --batch

# POST injection
sqlmap -u "http://target.com/login" --data="username=admin&password=pass" --batch

# With cookie
sqlmap -u "http://target.com/?id=1" --cookie="session=abc123" --batch

# Dump database
sqlmap -u "http://target.com/?id=1" --dump --batch

# Specific database
sqlmap -u "http://target.com/?id=1" -D database_name --dump --batch

# Shell
sqlmap -u "http://target.com/?id=1" --os-shell --batch
```

### Volatility 3 (Memory Forensics)
```bash
# Info
vol -f memory.dmp windows.info

# Processes
vol -f memory.dmp windows.pslist
vol -f memory.dmp windows.pstree
vol -f memory.dmp windows.psscan

# Network
vol -f memory.dmp windows.netscan

# Command history
vol -f memory.dmp windows.cmdline
vol -f memory.dmp windows.consoles

# DLLs
vol -f memory.dmp windows.dlllist

# Malware detection
vol -f memory.dmp windows.malfind

# Credential extraction
vol -f memory.dmp windows.hashdump
vol -f memory.dmp windows.lsadump

# Timeline
vol -f memory.dmp timeliner
```

### EvtxECmd (Windows Event Logs)
```bash
# Parse all logs
EvtxECmd.exe -f *.evtx --csv output/ --csvf timeline.csv

# Parse specific log
EvtxECmd.exe -f Security.evtx --csv output/ --csvf security.csv

# Key Event IDs:
# 4624 - Successful logon
# 4625 - Failed logon
# 4688 - Process creation
# 7045 - Service installation
# 4698 - Scheduled task
# 1102 - Log cleared
```

### tshark (Network Analysis)
```bash
# Protocol hierarchy
tshark -r capture.pcap -q -z io,phs

# HTTP requests
tshark -r capture.pcap -Y "http.request"

# Export objects
tshark -r capture.pcap --export-objects http,exported_files/

# Follow stream
tshark -r capture.pcap -q -z follow,tcp,ascii,0

# DNS queries
tshark -r capture.pcap -Y "dns.qry.name" -T fields -e dns.qry.name

# Suspicious traffic
tshark -r capture.pcap -Y "dns.qry.name.len > 50"
tshark -r capture.pcap -Y "http.user_agent contains 'python'"
```

### exiftool (Metadata)
```bash
# All metadata
exiftool file.jpg

# Specific field
exiftool -Comment file.jpg

# Write metadata
exiftool -Comment="flag{test}" file.jpg

# Batch
exiftool -r directory/

# GPS coordinates
exiftool -GPSLatitude -GPSLongitude file.jpg
```

### steghide (Steganography)
```bash
# Extract with empty password
steghide extract -sf file.jpg -f -p ""

# Extract with password
steghide extract -sf file.jpg -p "password"

# Embed
steghide embed -cf cover.jpg -ef secret.txt -p "password"

# Info
steghide info file.jpg
```

### binwalk (File Analysis)
```bash
# Scan
binwalk file.bin

# Extract
binwalk -e file.bin

# Extract recursively
binwalk -eM file.bin

# Specific signature
binwalk -R "\x89PNG" file.bin
```

### foremost (File Recovery)
```bash
# Recover files
foremost -i file.bin -o output/

# Specific types
foremost -t pdf,jpg,zip -i file.bin -o output/
```

### hashcat (Password Cracking)
```bash
# MD5
hashcat -m 0 hash.txt wordlist.txt

# SHA1
hashcat -m 100 hash.txt wordlist.txt

# SHA256
hashcat -m 1400 hash.txt wordlist.txt

# NTLM
hashcat -m 1000 hash.txt wordlist.txt

# With rules
hashcat -m 0 hash.txt wordlist.txt -r rules/best64.rule
```

### john (Password Cracking)
```bash
# Basic
john hash.txt

# Wordlist
john --wordlist=rockyou.txt hash.txt

# Specific format
john --format=raw-md5 hash.txt

# Show results
john --show hash.txt
```

### PE Studio (Malware Analysis)
```bash
# Run PE Studio
pestudio sample.exe

# Key sections:
# - Imports: Suspicious APIs
# - Strings: Extracted strings
# - Resources: Embedded files
# - Sections: Entropy/packing
```

### capa (Capability Detection)
```bash
# Detect capabilities
capa sample.exe

# Detailed output
capa -v sample.exe

# Custom rules
capa -r /path/to/rules/ sample.exe
```

### FLOSS (String Deobfuscation)
```bash
# Extract deobfuscated strings
floss sample.exe

# FLOSS finds:
# - Obfuscated strings
# - Stack strings
# - Tight strings
```

### olevba (VBA Macro Analysis)
```bash
# Extract VBA macros
olevba document.docm

# Deobfuscate
olevba --deobf document.docm
```

## Quick Reference Cards

### File Type Identification
```bash
file binary          # File type
xxd -l64 binary      # Hex dump header
strings binary       # Printable strings
binwalk binary       # Embedded files
exiftool image       # Image metadata
```

### Common Encodings
```bash
# Base64
echo "data" | base64 -d

# Hex
echo "data" | xxd -r -p

# URL
python3 -c "import urllib.parse; print(urllib.parse.unquote('data'))"

# ROT13
echo "data" | tr A-Za-z N-ZA-M
```

### Network Tools
```bash
# Port scan
nmap -sV target

# Banner grab
nc target port

# HTTP headers
curl -I http://target.com

# DNS lookup
dig target.com
nslookup target.com
```

### Crypto Tools
```bash
# OpenSSL
openssl enc -aes-256-cbc -d -in encrypted.bin -out decrypted.bin -k password

# RSA
openssl rsa -in private.pem -text -noout

# Hash
echo -n "data" | md5sum
echo -n "data" | sha1sum
```

## Automation Helpers

### CTFd API Client
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
    
    def submit_flag(self, chal_id, flag):
        r = requests.post(f"{self.base}/challenges/{chal_id}/attempt",
                         headers=self.headers,
                         json={"submission": flag})
        return r.json()
```

### Bulk File Analyzer
```bash
#!/bin/bash
for f in *; do
    echo "=== $f ==="
    file "$f"
    strings -n8 "$f" | head -3
    echo ""
done
```

### Quick Web Scanner
```bash
#!/bin/bash
URL=$1
echo "=== Headers ===" && curl -sI $URL
echo "=== robots.txt ===" && curl -s $URL/robots.txt
echo "=== .git ===" && curl -s $URL/.git/config
echo "=== admin ===" && curl -s -o /dev/null -w '%{http_code}' $URL/admin
```

### Quick Binary Check
```bash
#!/bin/bash
FILE=$1
echo "=== File ===" && file $FILE
echo "=== Strings ===" && strings -n8 $FILE | head -10
echo "=== Functions ===" && objdump -d $FILE | grep '<.*@plt>' | head -10
echo "=== Protections ===" && checksec --file=$FILE 2>/dev/null
```

## Speed Metrics
```
Tool selection time: <30 seconds
Basic analysis: <2 minutes
Tool execution: <5 minutes
Full analysis: <15 minutes
```
