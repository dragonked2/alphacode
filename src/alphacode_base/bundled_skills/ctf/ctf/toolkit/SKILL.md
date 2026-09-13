# CTF Toolkit — Competition-Grade Arsenal

## Tool Selection (<10s)

```
Binary → pwntools, checksec, ROPgadget, ropper, gdb, GEF/pwndbg
Web → curl, ffuf, sqlmap, nikto, hydra, wfuzz, nuclei
Crypto → python3, hashcat, john, openssl, sage
Rev → Ghidra, radare2, angr, z3, dnSpy, jadx
Forensics → tshark, binwalk, exiftool, steghide, foremost, zsteg
Misc → CyberChef, zbarimg, tesseract, multimon-ng
```

## Complete Installation Script

```bash
#!/bin/bash
echo "[*] System packages..."
sudo apt update && sudo apt install -y \
  steghide binwalk foremost exiftool tshark sqlmap ffuf hydra john hashcat \
  nikto wfuzz gdb python3-pip git curl wget unzip radare2 nmap \
  zbar-tools tesseract-ocr netcat socat default-jre

echo "[*] Python tools..."
pip install --user pwntools pycryptodome z3-solver angr capstone keystone \
  ropper ropgadget requests beautifulsoup4 scapy cryptography Pillow opencv-python-headless

echo "[*] Go tools..."
go install github.com/ffuf/ffuf/v2@latest 2>/dev/null
go install github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest 2>/dev/null

echo "[*] Wordlists..."
sudo apt install -y seclists 2>/dev/null || \
  git clone --depth 1 https://github.com/danielmiessler/SecLists.git /usr/share/seclists
echo "[+] Done. Run tool_check to verify."
```

## Tool Version Checking

```bash
#!/bin/bash
echo "=== CTF Tool Audit ==="
check_tool() {
  command -v "$1" >/dev/null 2>&1 && \
    echo "[OK] $1 → $(${1} --version 2>&1 | head -1)" || echo "[MISSING] $1"
}
for tool in python3 gdb curl wget nmap sqlmap hydra john hashcat \
  ffuf nikto wfuzz steghide binwalk exiftool tshark radare2 jq; do
  check_tool "$tool"
done
echo "--- Python ---"
pip list 2>/dev/null | grep -iE 'pwntools|pycryptodome|z3|angr|capstone|ropper|requests|scapy' || echo "pip not found"
```

## One-Shot Scripts

### Quick Recon
```bash
FILE=$1
echo "=== file ===" && file $FILE
echo "=== strings ===" && strings -n8 $FILE | head -20
echo "=== flag ===" && strings $FILE | grep -iE 'flag\{|ctf\{'
echo "=== hex ===" && xxd -l128 $FILE
```

### Quick Web — Full Recon
```bash
URL=$1
echo "=== headers ===" && curl -sI $URL | grep -iE 'server|x-powered|x-llm|x-agent'
echo "=== robots ===" && curl -s $URL/robots.txt 2>/dev/null | head -20
echo "=== common ===" && for f in flag flag.txt .git/config .env admin backup.zip .htaccess; do
  code=$(curl -s -o /dev/null -w '%{http_code}' $URL/$f); [ "$code" != "404" ] && echo "  $f → $code"
done
echo "=== dir brute ===" && ffuf -u $URL/FUZZ -w /usr/share/seclists/Discovery/Web-Content/common.txt -mc 200 -s 2>/dev/null | head -15
```

### SQL Injection Auto-Detect
```bash
URL=$1
echo "=== param fuzz ==="
ffuf -u "$URL/FUZZ" -w /usr/share/seclists/Discovery/Web-Content/common.txt -mc 200 -s 2>/dev/null | head -5
echo "=== sqlmap ==="
sqlmap -u "$URL" --batch --risk=2 --level=3 --threads=4 2>/dev/null | grep -iE 'parameter|flag|table|database'
echo "=== sqlmap POST ==="
echo "username=admin'--&password=x" | sqlmap -u "$URL/login" --data=@- --batch 2>/dev/null | grep -iE 'flag|inject'
```

### Binary One-Shot Pwn
```bash
FILE=$1
file $FILE; checksec --file=$FILE 2>/dev/null || readelf -l $FILE | grep GNU_STACK
strings $FILE | grep -iE 'flag\{|password|key|admin'
objdump -p $FILE 2>/dev/null | grep NEEDED
ROPgadget --binary $FILE 2>/dev/null | grep "pop rdi" | head -5
strings $FILE | grep '%p\|%x\|%s\|%n'
```

### Quick Crypto
```bash
FILE=$1
python3 -c "
import sys; d=open('$FILE','rb').read()
for k in range(256):
  r=bytes([b^k for b in d])
  if b'flag' in r.lower(): print(f'XOR Key:{k} → {r[:100]}')
"
```

### Quick Steg
```bash
FILE=$1
exiftool $FILE 2>/dev/null; binwalk $FILE
for p in "" "password" "1234" "admin" "flag"; do
  steghide extract -sf $FILE -f -p "$p" 2>/dev/null && echo "PASS: $p" && break
done
zsteg $FILE 2>/dev/null | head -10; tesseract $FILE stdout 2>/dev/null | head -5
```

### Quick PCAP
```bash
FILE=$1
capinfos $FILE; tshark -r $FILE -q -z io,phs
tshark -r $FILE --export-objects http,/tmp/http_exp 2>/dev/null
tshark -r $FILE -Y "dns.qry.name" -T fields -e dns.qry.name | sort -u
strings $FILE | grep -i 'flag{'
```

## Parallel Execution Templates

```bash
# Parallel challenge solving
solve_one() { cd "$1" && # add solve logic && echo "[DONE] $1" >> ../results.txt; }
export -f solve_one
ls -d */ | parallel -j4 'solve_one {}'

# Parallel web fuzzing
for url in "$@"; do
  (ffuf -u "$url/FUZZ" -w /usr/share/seclists/Discovery/Web-Content/common.txt -mc 200 -s &
   curl -s "$url/robots.txt" > "$url-robots.txt") &
done; wait

# Parallel XOR crack (16 cores)
python3 -c "
import multiprocessing as mp
def try_key(k):
  d=open('$FILE','rb').read(); r=bytes([b^k for b in d])
  return (k,r) if b'flag' in r.lower() else None
with mp.Pool(16) as pool:
  for r in pool.map(try_key, range(256)):
    if r: print(f'Key:{r[0]} → {r[1][:100]}')
"

# Parallel password crack
john --wordlist=/usr/share/seclists/Passwords/Leaked-Databases/rockyou.txt --fork=4 hash.txt
```

## Speed Hacks

```bash
# One-liner flag hunt
grep -rnEi 'flag\{[^}]+\}' . 2>/dev/null

# Base64 chain decode
echo "data" | base64 -d | base64 -d | base64 -d 2>/dev/null | grep -i flag

# XOR single byte brute
python3 -c "import sys; d=bytes.fromhex(sys.argv[1]); [print(f'Key:{k} → {bytes([b^k for b in d])}') for k in range(256) if b'flag' in bytes([b^k for b in d]).lower()]" "HEXDATA"

# XOR with known key
python3 -c "import sys; d=bytes.fromhex(sys.argv[1]); k=sys.argv[2].encode(); print(bytes([d[i]^k[i%len(k)] for i in range(len(d))]))" "HEXDATA" "SECRETKEY"
```

## Speed Metrics

```
Tool selection: <10s  |  Basic analysis: <1min  |  Full solve: <10min
```
