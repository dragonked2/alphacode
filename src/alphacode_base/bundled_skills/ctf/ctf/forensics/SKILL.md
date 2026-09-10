# CTF Forensics Skill

## Speed-First Approach: Solve forensics challenges in <10 minutes

### Phase 1: Instant Classification (<2 minutes)
```bash
FILE=$1

# Basic info
file $FILE
ls -la $FILE

# Quick strings (flag patterns)
strings $FILE | grep -iE 'flag\{|ctf\{|CTF\{|FLAG\{'

# Entropy check (high = encrypted/compressed)
python3 -c "
import math
data=open('$FILE','rb').read()
freq=[data.count(bytes([i]))/len(data) for i in range(256)]
e=-sum(f*math.log2(f) for f in freq if f>0)
print(f'Entropy: {e:.2f} bits/byte')
"

# File structure
xxd -l64 $FILE
binwalk $FILE
```

### Phase 2: Tool Selection (<1 minute)
```
MATCH file to tool:
├── .pcap/.pcapng → tshark, wireshark, NetworkMiner, Zui(Brim)
├── .jpg/.png/.gif → steghide, zsteg, exiftool, stegsolve
├── .wav/.mp3 → steghide, SpectralView, Sonic Visualiser, multimon-ng
├── .pdf → pdfid, pdftk, qpdf, pdf-parser
├── .doc/.docx → oletools, olevba, docx2txt
├── .zip/.rar/.7z → 7z, unzip, unrar, zipdetails
├── .gz/.tar/.bz2 → tar, gzip, bzip2
├── .raw/.dd → autopsy, binwalk, foremost
├── .vmdk/.vdi → guestmount, qemu-nbd
├── .evtx → EvtxECmd → Timeline Explorer
├── .dmp/.raw (memory) → Volatility 3, MemProcFS
├── .sqlite → DB Browser for SQLite
├── .pst/.ost → Thunderbird, Outlook Forensics Wizard
├── .log files → grep, text analysis, timeline
└── Registry hives → Registry Explorer, RegRipper
```

### Phase 3: Solve and Submit
```
IF flag found → submit immediately
ELSE → try next tool (come back later if stuck)
```

## One-Liner Solvers

### Quick File Analysis
```bash
FILE=$1
echo "=== File Type ===" && file $FILE
echo "=== Strings (flag) ===" && strings $FILE | grep -i 'flag{'
echo "=== Strings (long) ===" && strings -n8 $FILE | head -20
echo "=== Binwalk ===" && binwalk $FILE
echo "=== Exiftool ===" && exiftool $FILE 2>/dev/null
echo "=== Hex (first 128 bytes) ===" && xxd -l128 $FILE
```

### Quick Steganography Check
```bash
FILE=$1
echo "=== Exiftool ===" && exiftool $FILE
echo "=== Strings ===" && strings -n8 $FILE | head -20
echo "=== Binwalk ===" && binwalk $FILE
echo "=== Steghide (empty password) ===" && steghide extract -sf $FILE -f -p "" 2>/dev/null
echo "=== Zsteg (PNG/BMP) ===" && zsteg $FILE 2>/dev/null | head -10
echo "=== Stegsolve check needed ==="
```

### Quick PCAP Analysis
```bash
FILE=$1
echo "=== Basic Info ===" && capinfos $FILE
echo "=== Protocol Hierarchy ===" && tshark -r $FILE -q -z io,phs
echo "=== HTTP Objects ===" && tshark -r $FILE --export-objects http,/tmp/pcap_http
echo "=== DNS Queries ===" && tshark -r $FILE -Y "dns.qry.name" -T fields -e dns.qry.name | sort -u
echo "=== Strings (flag) ===" && strings $FILE | grep -i 'flag{'
echo "=== Credential Search ===" && tshark -r $FILE -Y "http.request.method==POST" -T fields -e http.file_data
```

### Quick PDF Analysis
```bash
FILE=$1
echo "=== PDFID ===" && python3 pdfid.py $FILE
echo "=== Strings ===" && strings -n8 $FILE | head -20
echo "=== Extract Objects ===" && python3 pdf-parser.py --objects $FILE
echo "=== Extract Streams ===" && python3 pdf-parser.py --streams $FILE
```

### Quick ZIP Analysis
```bash
FILE=$1
echo "=== Info ===" && unzip -l $FILE
echo "=== Extract ===" && unzip -o $FILE -d extracted/
echo "=== Check for encryption ===" && unzip -l $FILE | grep -i 'encrypted'
echo "=== Check for nested files ===" && find extracted/ -type f | xargs file
echo "=== Hidden files ===" && find extracted/ -name ".*" -type f
```

## Automated Extraction Scripts

### One-Command Forensics Suite
```bash
#!/bin/bash
FILE=$1
OUTDIR="extracted_$(basename $FILE)"
mkdir -p $OUTDIR

# Extract strings
strings -n8 $FILE > $OUTDIR/strings.txt
strings $FILE | grep -iE 'flag\{|ctf\{' > $OUTDIR/flag_strings.txt

# Extract embedded files
binwalk -e $FILE -C $OUTDIR 2>/dev/null

# Extract metadata
exiftool $FILE > $OUTDIR/metadata.txt 2>/dev/null

# Extract from archives
unzip -o $FILE -d $OUTDIR/zip_extract 2>/dev/null
7z x $FILE -o$OUTDIR/7z_extract 2>/dev/null

# Extract from PDF
python3 pdf-parser.py --all $FILE > $OUTDIR/pdf_objects.txt 2>/dev/null

echo "Extraction complete. Check $OUTDIR/"
ls -la $OUTDIR/
```

### Network Forensics Suite
```bash
#!/bin/bash
FILE=$1
OUTDIR="pcap_$(basename $FILE .pcap)"
mkdir -p $OUTDIR

# Basic info
capinfos $FILE > $OUTDIR/capinfos.txt

# Protocol analysis
tshark -r $FILE -q -z io,phs > $OUTDIR/protocol_hierarchy.txt

# Export HTTP objects
tshark -r $FILE --export-objects http,$OUTDIR/http_objects 2>/dev/null

# Export DNS
tshark -r $FILE -Y "dns" -T fields -e dns.qry.name > $OUTDIR/dns_queries.txt

# Export files
tshark -r $FILE --export-objects smb,$OUTDIR/smb_objects 2>/dev/null
tshark -r $FILE --export-objects imf,$OUTDIR/email_objects 2>/dev/null

# Extract all strings
strings $FILE > $OUTDIR/all_strings.txt
strings $FILE | grep -iE 'flag\{|ctf\{' > $OUTDIR/flag_strings.txt

echo "Analysis complete. Check $OUTDIR/"
```

### Image Forensics Suite
```bash
#!/bin/bash
FILE=$1
OUTDIR="image_$(basename $FILE .jpg)"
mkdir -p $OUTDIR

# Metadata
exiftool $FILE > $OUTDIR/metadata.txt

# Extract strings
strings -n8 $FILE > $OUTDIR/strings.txt

# Check for embedded files
binwalk $FILE > $OUTDIR/binwalk.txt

# Try steghide with empty password
steghide extract -sf $FILE -f -p "" -xf $OUTDIR/steghide_empty.jpg 2>/dev/null

# Try zsteg (PNG/BMP)
zsteg $FILE > $OUTDIR/zsteg.txt 2>/dev/null

# Try stegsolve (manual check)
echo "Check $OUTDIR/stegsolve.png with Stegsolve"

# Try other tools
steghide extract -sf $FILE -f -p "password" 2>/dev/null
stegseek $FILE /usr/share/wordlists/rockyou.txt 2>/dev/null

echo "Analysis complete. Check $OUTDIR/"
```

## File Type Specific Approaches

### PCAP Analysis
```bash
FILE=$1

# Protocol hierarchy
tshark -r $FILE -q -z io,phs

# HTTP traffic
tshark -r $FILE -Y "http" -T fields -e http.request.full_uri -e http.file_data

# DNS queries (look for C2, tunneling)
tshark -r $FILE -Y "dns.qry.name" -T fields -e dns.qry.name | sort -u

# Export all files
tshark -r $FILE --export-objects http,/tmp/http_export
tshark -r $FILE --export-objects smb,/tmp/smb_export

# Follow TCP stream
tshark -r $FILE -q -z follow,tcp,ascii,0

# Find credentials (HTTP POST)
tshark -r $FILE -Y "http.request.method==POST" -T fields -e http.file_data

# Extract from specific protocols
tshark -r $FILE -Y "ftp" -T fields -e ftp.request.command -e ftp.request.arg
tshark -r $FILE -Y "telnet" -T fields -e telnet.data

# VoIP analysis
tshark -r $FILE -Y "sip" -T fields -e sip.Method
tshark -r $FILE -Y "rtp" -T fields -e rtp.payload

# Suspicious traffic patterns
tshark -r $FILE -Y "dns.qry.name.len > 50" -T fields -e dns.qry.name | sort -u  # DGA domains
tshark -r $FILE -Y "http.user_agent contains 'python'" -T fields -e http.user_agent  # Python HTTP client
tshark -r $FILE -Y "http.request.uri contains 'base64'" -T fields -e http.request.uri  # Encoded data
```

### Image Steganography
```bash
FILE=$1

# Metadata
exiftool $FILE
strings -n8 $FILE | head -20

# Steghide
steghide extract -sf $FILE -f -p ""
steghide extract -sf $FILE -p "password"

# Zsteg (PNG/BMP)
zsteg $FILE

# Stegsolve (manual)
echo "Load in Stegsolve and check: Bit planes, Data extract, Frame browser"

# F5-steganography
java Extract $FILE

# OutGuess
outguess -r $FILE output.txt

# Jsteg
jsteg reveal $FILE output.txt

# Sonic Visualiser (audio)
# Load audio, add spectrogram, look for hidden patterns

# Audio spectrogram analysis
sox $FILE -n spectrogram -o spectrogram.png  # Create spectrogram
# Look for flag text in spectrogram image
```

### PDF Analysis
```bash
FILE=$1

# PDF structure
python3 pdfid.py $FILE

# Extract objects
python3 pdf-parser.py --all $FILE

# Extract streams
python3 pdf-parser.py --objects --filter $FILE

# Check for JavaScript
python3 pdf-parser.py --objects --javascript $FILE

# Check for embedded files
python3 pdf-parser.py --objects --embedded $FILE

# Extract images
pdfimages -j $FILE extracted_images/

# Extract text
pdftotext $FILE output.txt

# Check for encryption
qpdf --check $FILE
```

### Archive Analysis
```bash
FILE=$1

# List contents
unzip -l $FILE

# Extract
unzip -o $FILE -d extracted/

# Check for encryption
unzip -l $FILE | grep -i "encrypted"

# Check for nested archives
find extracted/ -type f | xargs file | grep -i "zip\|rar\|7z\|gzip"

# Check for hidden files
find extracted/ -name ".*" -type f

# Extract with password
unzip -P "password" $FILE

# ZIP details
zipdetails $FILE

# ZIP slip vulnerability check
python3 -c "
import zipfile, sys
with zipfile.ZipFile(sys.argv[1]) as z:
    for name in z.namelist():
        if '..' in name:
            print(f'SLIP: {name}')
" $FILE
```

### Memory Dump Analysis
```bash
FILE=$1

# Volatility 3 analysis
vol -f $FILE windows.info
vol -f $FILE windows.pslist
vol -f $FILE windows.pstree
vol -f $FILE windows.netscan
vol -f $FILE windows.cmdline
vol -f $FILE windows.consoles
vol -f $FILE windows.dlllist
vol -f $FILE windows.handles
vol -f $FILE windows.malfind
vol -f $FILE windows.hashdump
vol -f $FILE windows.lsadump
vol -f $FILE timeliner

# MemProcFS (mount as filesystem)
memprocfs -device $FILE -mount /tmp/memproc
ls /tmp/memproc/pid_*/

# Find strings
strings $FILE | grep -iE 'flag\{|ctf\{|password|key'

# Find processes
vol -f $FILE windows.pslist | grep -i "cmd\|powershell\|chrome\|firefox"
```

### Event Log Analysis
```bash
FILE=$1

# Parse with EvtxECmd
EvtxECmd.exe -f $FILE --csv output/ --csvf timeline.csv

# Key Event IDs
# 4624 - Successful logon
# 4625 - Failed logon (brute force)
# 4688 - Process creation
# 7045 - Service installation
# 4698 - Scheduled task
# 1102 - Log cleared

# Quick search
grep -i "4625" $FILE  # Brute force
grep -i "4688" $FILE  # Process creation
grep -i "7045" $FILE  # Service installation
grep -i "1102" $FILE  # Log cleared
```

## Common Patterns

### Pattern: Hidden in Metadata
```
Detection: Exiftool shows unusual fields
Attack:
1. Check Comment, UserComment, ImageDescription fields
2. Check GPS coordinates for flag location
3. Check Artist, Copyright fields
4. Check IPTC/XMP data
```

### Pattern: LSB Steganography
```
Detection: Image looks normal but zsteg finds data
Attack:
1. Use zsteg for PNG/BMP
2. Use stegsolve to view bit planes
3. Check LSB of each channel (R, G, B, A)
4. Try different bit orders (MSB/LSB)
```

### Pattern: File Carving
```
Detection: Binwalk finds embedded files
Attack:
1. Use binwalk -e to extract
2. Use foremost for deleted files
3. Check file headers for magic bytes
4. Try different carving tools
```

### Pattern: Network Extraction
```
Detection: PCAP contains file transfers
Attack:
1. Export HTTP objects
2. Export SMB objects
3. Follow TCP streams
4. Check DNS for tunneling
5. Extract from VoIP calls
```

### Pattern: Hidden in Plain Sight
```
Detection: File contains obvious but obfuscated data
Attack:
1. Check for base64 strings
2. Check for hex-encoded data
3. Check for XOR encoding
4. Check for simple ciphers (ROT13, etc.)
```

### Pattern: Ransomware Artifacts
```
Detection: .locked/.encrypted extensions, ransom notes
Attack:
1. Check for shadow copy deletion commands
2. Check event logs for mass file modifications
3. Extract encryption keys from memory
4. Analyze ransomware sample for weaknesses
```

### Pattern: Malware C2 Communication
```
Detection: Unusual DNS queries, HTTP POST with encoded data
Attack:
1. Extract DGA domains from PCAP
2. Decode Base64/Hex encoded data in HTTP
3. Identify C2 protocol patterns
4. Extract IOCs for threat intel
```

## Speed Metrics
```
Average solve times (target):
- Basic steganography: <3 minutes
- PCAP file extraction: <5 minutes
- PDF analysis: <5 minutes
- Memory dump basics: <10 minutes
- Complex steg: <15 minutes
- Event log triage: <5 minutes
```
