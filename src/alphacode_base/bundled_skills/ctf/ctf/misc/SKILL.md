# CTF Misc Skill

## Speed-First Approach: Solve misc challenges in <10 minutes

### Phase 1: Instant Classification (<2 minutes)
```bash
# Run this immediately on any misc challenge
FILE=$1

# Basic info
file $FILE
strings $FILE | grep -iE 'flag\{|ctf\{|CTF\{|FLAG\{'

# Entropy check
python3 -c "
import math
data=open('$FILE','rb').read()
freq=[data.count(bytes([i]))/len(data) for i in range(256)]
e=-sum(f*math.log2(f) for f in freq if f>0)
print(f'Entropy: {e:.2f} bits/byte')
"

# File structure
xxd -l64 $FILE
ls -la $FILE
```

### Phase 2: Pattern Recognition (<3 minutes)
```
MATCH challenge to known pattern:
├── QR code / Barcode → zbarimg, online readers
├── Audio file → Sonic Visualiser, SpectralView, SSTV
├── Image with text → OCR (tesseract), steganography
├── Encoded text → CyberChef, base64, hex, ROT13
├── Logic puzzle → Write solver script
├── Network challenge → nmap, netcat, Wireshark
├── OSINT → Google Dorking, social media search
└── Programming challenge → Write solve script
```

### Phase 3: Solve and Submit
```
IF flag found → submit immediately
ELSE → try next tool (come back later if stuck)
```

## One-Liner Solvers

### Quick Decoding
```bash
# Base64
echo "eyJmbGFnIjoiVEVTVCJ9" | base64 -d

# Hex
echo "48656c6c6f" | xxd -r -p

# ROT13
echo "synt{grfg}" | tr A-Za-z N-ZA-M

# Binary
python3 -c "print(int('0100100001101001',2).to_bytes(2,'big').decode())"

# URL decode
python3 -c "import urllib.parse; print(urllib.parse.unquote('flag%7Btest%7D'))"
```

### Quick Image Analysis
```bash
# QR code
zbarimg $FILE

# OCR
tesseract $FILE output
cat output.txt

# Metadata
exiftool $FILE

# Steg check
steghide extract -sf $FILE -f -p "" 2>/dev/null
zsteg $FILE 2>/dev/null | head -10
```

### Quick Audio Analysis
```bash
# File info
file $FILE
exiftool $FILE

# Strings
strings $FILE | head -20

# Check for SSTV
# Use QSSTV or RX-SSTV to decode

# Check for DTMF
# Use multimon-ng
```

## Common Misc Patterns

### QR Code / Barcode
```bash
# Decode QR code
zbarimg $FILE

# Or use online: https://zxing.org/w/decode.jspx

# Multiple QR codes
zbarimg --raw $FILE

# Encode new QR code
qrencode -o output.png "FLAG{test}"
```

### Audio Steganography (SSTV)
```bash
# Slow Scan TV (images in audio)
# Use QSSTV or RX-SSTV
qsstv  # Load audio file, decode

# Or use multimon-ng
multimon-ng -t wav -a SSTV $FILE

# DTMF tones
multimon-ng -t wav -a DTMF $FILE
```

### Audio Spectrogram
```bash
# Create spectrogram
sox $FILE -n spectrogram -o spectrogram.png

# Or use Audacity
# Load audio → Analyze → Spectrogram

# Look for hidden patterns in spectrogram
# Flag often appears as text in spectrogram
```

### Binary Data in Images
```bash
# Extract binary data from image
python3 -c "
from PIL import Image
img = Image.open('$FILE')
data = img.tobytes()
print(data[:100])
"

# Extract LSB
python3 -c "
from PIL import Image
img = Image.open('$FILE')
pixels = list(img.getdata())
bits = ''.join([str(p[0] & 1) for p in pixels])
print(''.join([chr(int(bits[i:i+8], 2)) for i in range(0, len(bits)-7, 8)]))
"
```

### Network Challenges
```bash
# Port scan
nmap -sV $TARGET

# Connect
nc $TARGET $PORT

# Banner grab
echo "" | nc $TARGET $PORT

# HTTP
curl -v http://$TARGET:$PORT
curl -v http://$TARGET:$PORT/robots.txt
```

### Programming Challenges
```python
# Common patterns and solvers

# 1. XOR decode
def xor_decode(data, key):
    return bytes([b ^ key[i % len(key)] for i, b in enumerate(data)])

# 2. Base64 chain
import base64
def base64_chain(data, n):
    for _ in range(n):
        data = base64.b64decode(data)
    return data

# 3. Custom hash
def custom_hash(s):
    h = 0
    for c in s:
        h = (h * 31 + ord(c)) & 0xFFFFFFFF
    return h

# 4. Random with seed
import random
random.seed(12345)  # Find the seed
for _ in range(10):
    print(random.randint(0, 100))
```

### OSINT Challenges
```bash
# Google dorking
site:example.com filetype:pdf
intitle:"index of" password
inurl:admin login

# Image reverse search
# Use Google Images, TinEye, Yandex

# Social media
# Check Facebook, Twitter, Instagram, LinkedIn

# Email OSINT
# HaveIBeenPwned, hunter.io, emailrep.io
```

### Cryptography Misc
```bash
# Common ciphers
# ROT13
echo "synt{grfg}" | tr A-Za-z N-ZA-M

# Atbash
python3 -c "
s = 'synt{grfg}'
print(''.join(chr(219-ord(c)) if c.isalpha() else c for c in s))
"

# Rail fence
python3 -c "
def rail_fence_decrypt(cipher, key):
    n = len(cipher)
    rail = [['\n' for _ in range(n)] for _ in range(key)]
    dir_down = False
    row, col = 0, 0
    for i in range(n):
        if row == 0 or row == key - 1:
            dir_down = not dir_down
        rail[row][col] = '*'
        col += 1
        row += 1 if dir_down else -1
    index = 0
    for i in range(key):
        for j in range(n):
            if rail[i][j] == '*' and index < n:
                rail[i][j] = cipher[index]
                index += 1
    result = []
    row, col = 0, 0
    for i in range(n):
        if row == 0 or row == key - 1:
            dir_down = not dir_down
        result.append(rail[row][col])
        col += 1
        row += 1 if dir_down else -1
    return ''.join(result)

print(rail_fence_decrypt('WECRLTEERDSOEEFEAOCAIVDEN', 4))
"
```

## Automated Solve Scripts

### QR Code Solver
```bash
#!/bin/bash
# Solve QR code challenges
FILE=$1
echo "=== Decode QR ==="
zbarimg --raw $FILE
```

### Audio SSTV Solver
```bash
#!/bin/bash
# Solve SSTV challenges
FILE=$1
echo "=== Decode SSTV ==="
qsstv &
echo "Load $FILE in QSSTV"
```

### Network Challenge Solver
```bash
#!/bin/bash
# Solve network challenges
TARGET=$1
PORT=$2
echo "=== Scan ==="
nmap -sV $TARGET
echo "=== Connect ==="
nc -v $TARGET $PORT
echo "=== Banner ==="
echo "" | nc $TARGET $PORT
```

### Programming Challenge Solver
```python
#!/usr/bin/env python3
# Solve common programming challenges

import sys
import base64
import hashlib
import random

def solve(args):
    # Base64 decode
    if args[1] == "b64":
        print(base64.b64decode(args[2]).decode())
    
    # XOR decode
    elif args[1] == "xor":
        data = bytes.fromhex(args[2])
        key = int(args[3])
        print(bytes([b ^ key for b in data]).decode())
    
    # Hash crack
    elif args[1] == "hash":
        target = args[2]
        wordlist = args[3] if len(args) > 3 else "/usr/share/wordlists/rockyou.txt"
        with open(wordlist) as f:
            for line in f:
                word = line.strip()
                if hashlib.md5(word.encode()).hexdigest() == target:
                    print(f"Found: {word}")
                    break
    
    # Random with seed
    elif args[1] == "random":
        seed = int(args[2])
        random.seed(seed)
        for _ in range(10):
            print(random.randint(0, 100))

if __name__ == "__main__":
    solve(sys.argv)
```

## Speed Metrics
```
Average solve times (target):
- QR code decode: <1 minute
- Simple encoding: <2 minutes
- Audio SSTV: <5 minutes
- Spectrogram analysis: <5 minutes
- Programming challenge: <10 minutes
- OSINT: <15 minutes
```
