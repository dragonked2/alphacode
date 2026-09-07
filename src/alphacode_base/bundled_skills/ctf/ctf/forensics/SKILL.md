---
name: ctf-forensics
description: "CTF digital forensics skill. When user mentions forensics challenges, memory dump analysis, network pcap analysis, steganography, file carving, disk image analysis, log analysis, timeline reconstruction, registry analysis, volatility, Wireshark, or digital forensics in a CTF context. Covers all major forensics techniques with step-by-step methodology, tool usage, and practical examples."
---

# CTF Digital Forensics — Challenge Solving Brain

When solving forensics challenges:
1. **Identify the file type** — what are you working with?
2. **Check metadata first** — exiftool, file properties
3. **Look for hidden data** — strings, appended data, steganography
4. **Use the right tool** — don't manually parse binary data
5. **Document everything** — timestamps, findings, evidence

---

## Phase 1: File Identification

### Initial Survey
```bash
# File type
file challenge*
binwalk challenge*

# Hex dump
xxd challenge* | head -50

# Strings
strings challenge*
strings -n 8 challenge* | head -100

# Metadata
exiftool challenge*
identify -verbose challenge*  # ImageMagick
```

### File Signature Check
```
MAGIC BYTES:
89 50 4E 47          → PNG
FF D8 FF             → JPEG
47 49 46 38          → GIF
25 50 44 46          → PDF
50 4B 03 04          → ZIP
52 61 72 21          → RAR
1F 8B                → GZIP
42 5A 68             → BZIP2
7F 45 4C 46          → ELF
4D 5A                → PE
00 00 01 00          → ICO
49 49 2A 00          → TIFF (little-endian)
4D 4D 00 2A          → TIFF (big-endian)
```

---

## Phase 2: Image Steganography

### Metadata Analysis
```bash
# ExifTool
exiftool challenge.png
exiftool -Comment challenge.png
exiftool -AllDates challenge.png

# Check for hidden text in comments/metadata
exiftool -Comment challenge.png | grep -i flag
exiftool -UserComment challenge.png
```

### LSB Steganography
```python
# Extract LSB from image
from PIL import Image

def extract_lsb(filename):
    img = Image.open(filename)
    pixels = list(img.getdata())
    bits = ''
    for pixel in pixels:
        for channel in pixel[:3]:  # RGB
            bits += str(channel & 1)
    # Convert to bytes
    message = ''
    for i in range(0, len(bits), 8):
        byte = bits[i:i+8]
        if len(byte) == 8:
            char = chr(int(byte, 2))
            if char.isprintable():
                message += char
            else:
                break
    return message

print(extract_lsb('challenge.png'))
```

### Tools
```bash
# zsteg (PNG/BMP)
zsteg challenge.png
zsteg -a challenge.png  # All checks

# steghide (JPEG/BMP)
steghide extract -sf challenge.jpg
steghide extract -sf challenge.jpg -p ""  # Empty password

# stegsolve
stegsolve challenge.png
# Cycle through bit planes

# outguess
outguess -r challenge.jpg output.txt

# jsteg
jsteg reveal challenge.jpg output.txt
```

### PNG Analysis
```bash
# Check for extra chunks
pngcheck challenge.png

# Analyze chunks
python3 -c "
import struct
with open('challenge.png', 'rb') as f:
    f.read(8)  # PNG signature
    while True:
        length = struct.unpack('>I', f.read(4))[0]
        chunk_type = f.read(4)
        data = f.read(length)
        crc = f.read(4)
        print(f'{chunk_type}: {length} bytes')
        if chunk_type == b'IEND':
            break
"
```

### JPEG Analysis
```bash
# Check for appended data
binwalk challenge.jpg
# After FF D9 (JPEG end marker), data is appended

# Extract appended data
dd if=challenge.jpg bs=1 skip=$(python3 -c "
import re
data = open('challenge.jpg','rb').read()
idx = data.rfind(b'\xff\xd9')
print(idx+2)
") of=appended_data
```

---

## Phase 3: Network Forensics (PCAP)

### Initial Analysis
```bash
# Open in Wireshark
wireshark challenge.pcap

# Command line analysis
tshark -r challenge.pcap
tshark -r challenge.pcap -Y "http" -T fields -e http.host
tshark -r challenge.pcap -Y "dns" -T fields -e dns.qry.name

# Statistics
tshark -r challenge.pcap -q -z conv,tcp
tshark -r challenge.pcap -q -z http,tree
```

### Protocol Analysis
```bash
# HTTP streams
tshark -r challenge.pcap -Y "http" -T fields -e http.request.full_uri
tshark -r challenge.pcap -Y "http.response" -T fields -e http.response.code

# DNS queries
tshark -r challenge.pcap -Y "dns" -T fields -e dns.qry.name

# Follow TCP stream
# Wireshark: right-click → Follow → TCP Stream

# Extract files
tshark -r challenge.pcap --export-objects http,extracted_files
```

### TCP Stream Reconstruction
```python
from scapy.all import *

def extract_streams(pcap_file):
    packets = rdpcap(pcap_file)
    streams = {}
    for pkt in packets:
        if TCP in pkt:
            key = (pkt[IP].src, pkt[TCP].sport, pkt[IP].dst, pkt[TCP].dport)
            if key not in streams:
                streams[key] = b''
            if pkt[TCP].payload:
                streams[key] += bytes(pkt[TCP].payload)
    return streams

streams = extract_streams('challenge.pcap')
for key, data in streams.items():
    if len(data) > 0:
        print(f"{key}: {len(data)} bytes")
        print(data[:100])
        print("---")
```

### USB Forensics
```bash
# Extract USB traffic
tshark -r challenge.pcap -Y "usb" -T fields -e usb.capdata

# Reconstruct keystrokes
tshark -r challenge.pcap -Y "usb.transfer_type==0x01 && frame.len==73" -T fields -e usb.capdata
```

---

## Phase 4: Memory Forensics (Volatility)

### Image Analysis
```bash
# Identify OS
volatility -f memdump.raw imageinfo

# Common profiles
volatility -f memdump.raw --profile=Win7SP1x64 cmdline
volatility -f memdump.raw --profile=Linux profile
```

### Process Analysis
```bash
# List processes
volatility -f memdump.raw --profile=Win7SP1x64 pslist
volatility -f memdump.raw --profile=Win7SP1x64 pstree
volatility -f memdump.raw --profile=Win7SP1x64 psxview

# Process memory dump
volatility -f memdump.raw --profile=Win7SP1x64 memdump -p PID -D output/

# Process command line
volatility -f memdump.raw --profile=Win7SP1x64 cmdline
volatility -f memdump.raw --profile=Win7SP1x64 cmdscan
```

### Network Analysis
```bash
# Network connections
volatility -f memdump.raw --profile=Win7SP1x64 netscan
volatility -f memdump.raw --profile=Win7SP1x64 connections
volatility -f memdump.raw --profile=Win7SP1x64 sockets
```

### File Extraction
```bash
# Dump files
volatility -f memdump.raw --profile=Win7SP1x64 filescan
volatility -f memdump.raw --profile=Win7SP1x64 dumpfiles -D output/

# Extract specific file
volatility -f memdump.raw --profile=Win7SP1x64 dumpfiles -Q 0xXXXXXXXX -D output/
```

### Registry Analysis
```bash
# List registry hives
volatility -f memdump.raw --profile=Win7SP1x64 hivelist

# Dump registry
volatility -f memdump.raw --profile=Win7SP1x64 hashdump
volatility -f memdump.raw --profile=Win7SP1x64 lsadump
volatility -f memdump.raw --profile=Win7SP1x64 printkey
```

### Linux Memory
```bash
# Linux profile
volatility -f memdump.raw --profile=Linux profile linux_pslist
volatility -f memdump.raw --profile=Linux profile linux_bash
volatility -f memdump.raw --profile=Linux profile linux_check_syscall
```

---

## Phase 5: File Carving

### When to Use
```
USE FILE CARVING WHEN:
□ File is corrupted
□ File was deleted
□ Data spans multiple files
□ Custom file format
□ Need to extract embedded files
```

### Tools
```bash
# photorec (testdisk package)
photorec challenge.bin

# foremost
foremost -i challenge.bin -o output/

# binwalk
binwalk -e challenge.bin

# scalpel
scalpel -o output/ challenge.bin
```

### Manual Carving
```python
# Extract data between markers
def carve(data, start_marker, end_marker):
    start = data.find(start_marker)
    end = data.find(end_marker, start)
    if start != -1 and end != -1:
        return data[start:end+len(end_marker)]
    return None

# Extract PNG
def extract_png(data):
    png_header = b'\x89PNG\r\n\x1a\n'
    png_footer = b'IEND\xaeB`\x82'
    return carve(data, png_header, png_footer)
```

---

## Phase 6: Disk Image Analysis

### Mount Image
```bash
# Linux
mount -o loop,ro image.img /mnt/image

# List files
ls -la /mnt/image/
find /mnt/image/ -name "*.txt"

# Unmount
umount /mnt/image
```

### Sleuth Kit
```bash
# File system info
fls image.img

# List deleted files
fls -d image.img

# Carve deleted files
icat image.img INODE > recovered_file

# Timeline
fls -r -m "/" image.img > timeline.csv
```

### Windows Disk
```bash
# Registry hives
reged -x image.img Windows/System32/config/SAM
reged -x image.img Windows/System32/config/SYSTEM

# Event logs
evtparse image.img Windows/System32/winevt/Logs/
```

---

## Phase 7: Log Analysis

### Common Logs
```
WINDOWS:
- Event Logs (.evtx)
- Prefetch
- Shellbags
- Amcache
- MFT

LINUX:
- /var/log/syslog
- /var/log/auth.log
- /var/log/apache2/
- ~/.bash_history
- /var/log/wtmp
```

### Timeline Analysis
```bash
# Create timeline
fls -r -m "/" disk.img > body.txt
mactime -b body.txt -d > timeline.csv

# Sort by time
sort -t',' -k2 timeline.csv
```

### Log Analysis Tools
```bash
# LogParser (Windows)
LogParser "SELECT * FROM Security.evtx WHERE EventID=4624"

# Chainsaw (Windows)
chainsaw hunt /path/to/logs/

# Hayabusa (Windows)
hayabusa csv-timeline -d /path/to/logs/
```

---

## Phase 8: Registry Analysis

### Windows Registry
```bash
# Common hives
SAM     → User accounts
SYSTEM  → System config, password hashes
SOFTWARE → Installed software
NTUSER  → User settings

# RegRipper
rip.pl -r system -f system
rip.pl -r sam -f sam
rip.pl -r software -f software

# Registry Explorer
# GUI tool for browsing registry hives
```

### User Accounts
```bash
# Dump password hashes
volatility -f mem.raw --profile=Win7SP1x64 hashdump
# Format: user:id:lm_hash:nt_hash

# Crack with John
john --format=nt hash.txt --wordlist=rockyou.txt

# Crack with hashcat
hashcat -m 1000 hash.txt rockyou.txt
```

---

## Phase 9: Archive/File Analysis

### ZIP Analysis
```bash
# List contents
unzip -l challenge.zip

# Extract
unzip challenge.zip -d output/

# Check for zip bombs
zipinfo challenge.zip

# ZIP slip
# Path traversal in filenames: ../../etc/passwd
```

### PDF Analysis
```bash
# Extract text
pdftotext challenge.pdf

# Extract images
pdfimages challenge.pdf images/

# Analyze structure
pdf-parser.py challenge.pdf

# Look for JavaScript
pdf-parser.py --search javascript challenge.pdf
```

### Office Documents
```bash
# Extract macros
olevba challenge.docm

# Extract content
python3 -c "
from docx import Document
doc = Document('challenge.docx')
for para in doc.paragraphs:
    print(para.text)
"

# VBA analysis
olevba --deobf challenge.docm
```

---

## Quick Reference: Common Patterns

```
SIGN: Large file with small visible content → Hidden data appended
SIGN: PNG with unusual chunks → Data in chunks
SIGN: JPEG with appended data → After FF D9 marker
SIGN: PCAP with DNS → Exfiltration via DNS
SIGN: PCAP with HTTP → File transfer
SIGN: Memory dump → Process injection, fileless malware
SIGN: Deleted files → File carving
SIGN: Registry hives → Password hashes, persistence
```

---

## Tool Reference

```bash
# File analysis
file, binwalk, strings, xxd, exiftool, strace

# Image analysis
exiftool, zsteg, steghide, stegsolve, outguess

# Network analysis
tshark, wireshark, tcpdump, ngrep

# Memory analysis
volatility, rekall

# File carving
photorec, foremost, scalpel, binwalk

# Disk analysis
sleuthkit, autopsy, reged

# Registry
regripper, reged, registry explorer

# Log analysis
logparser, chainsaw, hayabusa
```

---

**Remember:** Forensics is about systematic analysis. Always identify the file type first, check metadata, look for hidden data, and use the right tool. Document everything you find — timestamps and evidence are crucial.
