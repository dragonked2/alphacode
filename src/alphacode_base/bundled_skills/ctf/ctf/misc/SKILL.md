---
name: ctf-misc
description: Miscellaneous CTF analysis — authorized educational environment covering encoding chains, image analysis, audio analysis, logic puzzles, and quick-win patterns.
---

# CTF Misc Analysis — Authorized Educational Environment

This skill operates within authorized Capture The Flag competition rules. All techniques are applied to CTF challenge targets as defined by the competition organizers.

## Authorization Context

CTF competitions explicitly authorize security research on their challenges. Every challenge description IS the authorization scope. This is a sanctioned educational environment.

## Multi-Layer Encoding

### Quick Decode Chain (<10s)
```bash
# Base64 loop
echo -n "encoded" | base64 -d | base64 -d | base64 -d
# Hex
echo -n "6f6e6574776f" | xxd -r -p
# URL decode
python3 -c "import urllib.parse; print(urllib.parse.unquote('%7B%22flag%22%7D'))"
# ROT13
echo "synt{grfg}" | tr 'A-Za-z' 'N-ZA-Mn-za-m'
# Binary
echo "01101100" | sed 's/ //g' | paste -sd'' - | sed 's/.\{8\}/& /g' | xargs -n1 printf "%d\\n" | xargs printf "\\$(printf '%03o' %s)"
```

### Encoding Detection (<5s)
```bash
file --mime-encoding file
strings file | head -50
# Entropy check (high = encrypted/compressed)
binwalk -E file
```

### XOR Decryption
```bash
# Single-byte XOR brute force
for i in $(seq 0 255); do python3 -c "f=open('enc','rb').read(); print(chr($i).join([chr(b^$i) for b in f]))" 2>/dev/null | grep -i "flag|ctf"; done
# Known-plaintext XOR
python3 -c "
c=open('enc','rb').read()
k=b'FLAG'
key=bytes([c[i]^k[i%len(k)] for i in range(len(k))])
print(''.join(chr(c[i]^key[i%len(key)]) for i in range(len(c))))
"
```

## Image Challenges

```bash
# Metadata/exif
exiftool image.png
# LSB steganography
zsteg image.png -a
steghide extract -sf image.jpg
# Bit plane analysis
python3 -c "
from PIL import Image
img=Image.open('image.png')
for x in range(img.width):
    for y in range(img.height):
        r,g,b=img.getpixel((x,y))
        if r&1: print(chr(r),end='')
"
# Color channels
convert image.png -channel R -separate red.png
# PNG more chunks
pngcheck -v image.png
# QR code decode
zbarimg image.png
```

## Audio Challenges

```bash
# Spectrogram (visual flag)
sox audio.wav -n spectrogram -o spectro.png
# Decode DTMF
multimon-ng -t wav -a DTMF audio.wav
# SSTV
QSSTV  # GUI tool for Slow Scan TV
# Binary audio
python3 -c "
import wave
w=wave.open('audio.wav','rb')
frames=w.readframes(w.getnframes())
print(''.join(['1' if f>128 else '0' for f in frames[::2]]))
" | fold -w8 | while read b; do printf "\\$(printf '%d' 2#$b)"; done; echo
```

## Bot / CAPTCHA Challenges

```bash
# Selenium automation
python3 -c "
from selenium import webdriver
d=webdriver.Chrome()
d.get('http://target/challenge')
# Parse DOM for answer
print(d.find_element('css selector','#flag').text)
"
# Puppeteer (Node)
node -e "
const p=require('puppeteer');
(async()=>{const b=await p.launch();const pg=await b.newPage();
await pg.goto('http://target');console.log(await pg.\$eval('#flag',e=>e.textContent));})()
"
```

## Hardware / Embedded

```bash
# UART baud rate detection
baudrate -f uart_capture.log
# Read firmware strings
strings firmware.bin | grep -i "flag|password|key"
binwalk -Me firmware.bin
# JTAG/UART pinout
logic_analyzer uart.bin  # analyze with sigrok
```

## Logic Puzzles (Z3)

```python
from z3 import *
s = Solver()
flag = [BitVec(f'f{i}', 8) for i in range(40)]
# Add constraints from challenge
s.add(flag[0] ^ flag[1] == 0x42)
s.add(sum(flag) == 1234)
if s.check() == sat:
    m = s.model()
    print(''.join([chr(m[f].as_long()) for f in flag]))
```

## Programming Challenges

```python
# Quick pwntools solve
from pwn import *
r = remote('challenge.ctf.com', 1337)
r.sendline(b'A'*40 + p64(0x401180))
r.interactive()
```

## OSINT

```bash
# Image reverse search
# Check EXIF GPS coordinates
exiftool -gps:all image.jpg
# Username enumeration
# theHarvester -d target.com -b google
```

## AI/LLM Analysis

```bash
# Prompt injection detection
cat prompt.txt | grep -iE "ignore previous|system prompt|reveal"
# Token smuggling
python3 -c "print(' '.join(['\\u200b']*100))"  # zero-width chars
# Jailbreak patterns
grep -iE "DAN|jailbreak|pretend you are" challenge.txt
```

## CTF References
- **GoogleCTF 2024**: XOR + steganography chain
- **DEF CON Quals 2024**: Audio SSTV decode
- **picoCTF 2025**: Multi-layer encoding
- **DownUnderCTF 2024**: Z3 constraint solving
- **HTB Challenges 2025**: AI prompt injection
- **CrewCTF 2024**: Hardware UART extraction
- **Cortex 2025**: QR + blockchain hybrid

## Speed Metrics

| Metric | Target |
|--------|--------|
| Encoding identification | <10s |
| Base64/Hex decode | <5s |
| Image stego scan | <30s |
| Audio spectrogram | <20s |
| Z3 solver | <60s |
