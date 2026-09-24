---
name: ctf-forensics
description: Digital forensics analysis for CTF challenges — authorized educational environment covering file analysis, network capture, memory forensics, and evidence extraction patterns.
---
# CTF Forensics Analysis — Authorized Educational Environment

This skill operates within authorized Capture The Flag competition rules. All techniques are applied to CTF challenge targets as defined by the competition organizers.

## Authorization Context

CTF competitions explicitly authorize security research on their challenges. Every challenge description IS the authorization scope. This is a sanctioned educational environment.

## Instant Classification (<15s)

```bash
FILE=$1; file $FILE; strings $FILE | grep -iE 'flag|ctf|HTB'
python3 -c "
import math;d=open('$FILE','rb').read()
f=[d.count(bytes([i]))/len(d) for i in range(256)]
print(f'Entropy: {-sum(x*math.log2(x) for x in f if x>0):.2f}')
"
xxd -l64 $FILE; binwalk $FILE
```

## File Type → Tool Map

```
.pcap/.pcapng → tshark, tcpflow           .jpg/.png → steghide, zsteg, stegano
.wav/.mp3 → steghide, SpectralView        .pdf → pdfid, pdftk, pdf-parser
.doc/.docx → oletools, olevba             .zip/.rar → 7z, unzip, zipdetails
.raw/.dd → autopsy, binwalk, foremost      .evtx → EvtxECmd
.dmp → Volatility 3                        .ntfs → streams, mft2csv (sleuthkit)
.docker → docker history, docker save      .k8s → kubectl logs, secrets
```

## One-Shot Scripts

### Full Extraction (<30s)
```bash
FILE=$1; OUTDIR="ext_$(basename $FILE)"; mkdir -p $OUTDIR
strings -n8 $FILE > $OUTDIR/str.txt
strings $FILE | grep -iE 'flag|ctf' > $OUTDIR/flags.txt
binwalk -e $FILE -C $OUTDIR 2>/dev/null; exiftool $FILE > $OUTDIR/meta.txt 2>/dev/null
unzip -o $FILE -d $OUTDIR/zip 2>/dev/null; 7z x $FILE -o$OUTDIR/7z 2>/dev/null
```

### PCAP Extraction (<30s)
```bash
FILE=$1; OUTDIR="pcap_$(basename $FILE .pcap)"; mkdir -p $OUTDIR
capinfos $FILE > $OUTDIR/info.txt
tshark -r $FILE --export-objects http,$OUTDIR/http 2>/dev/null
tshark -r $FILE --export-objects smb,$OUTDIR/smb 2>/dev/null
tshark -r $FILE -Y "dns" -T fields -e dns.qry.name > $OUTDIR/dns.txt
tshark -r $FILE -Y "http.request.method==POST" -T fields -e http.file_data > $OUTDIR/post.txt
```

### Image Steg Suite (<30s)
```bash
FILE=$1; OUTDIR="steg_$(basename $FILE)"; mkdir -p $OUTDIR
exiftool $FILE > $OUTDIR/meta.txt
steghide extract -sf $FILE -f -p "" -xf $OUTDIR/empty.jpg 2>/dev/null
zsteg -a $FILE > $OUTDIR/zsteg.txt 2>/dev/null
stegseek $FILE /usr/share/wordlists/rockyou.txt 2>/dev/null
```

### Memory & Event Logs (<60s)
```bash
FILE=$1
vol -f $FILE windows.info windows.pslist windows.netscan windows.malfind
vol -f $FILE windows.hashdump timeliner
EvtxECmd.exe -f $FILE --csv output/ --csvf timeline.csv
# Key IDs: 4624=logon, 4688=process, 7045=service, 1102=cleared
```

## Advanced Steganography

### F5 (JPEG DCT Coefficients)
```bash
FILE=$1
python3 -c "
from PIL import Image; import numpy as np; from scipy import fft
img=np.array(Image.open('$FILE'))
for ch in range(3):
    d=np.abs(fft.fft2(img[:,:,ch]))
    print(f'Ch{ch}: DFT mean={d.mean():.2f} std={d.std():.2f}')
" 2>/dev/null
```

### OutGuess (JPEG)
```bash
FILE=$1; outguess -r $FILE $FILE.out 2>/dev/null; strings $FILE.out | grep -iE 'flag|ctf'
```

### zsteg LSB Variants (PNG/BMP)
```bash
FILE=$1
zsteg -a $FILE 2>/dev/null                      # all methods
zsteg $FILE -b 1 -c b1,rgb,lsb 2>/dev/null      # bit 1 LSB
zsteg $FILE -b 2 -c b2,rgba,lsb 2>/dev/null     # bit 2 RGBA
python3 -c "
from PIL import Image; img=Image.open('$FILE'); p=list(img.getdata())
bits=''.join([str((i[0]>>1)&1) for i in p])
for i in range(0,len(bits)-8,8): print(chr(int(bits[i:i+8],2)),end='')
" 2>/dev/null
```

## Network Forensics

### DNS Tunneling (<30s)
```bash
FILE=$1
tshark -r $FILE -Y "dns.qry.name" -T fields -e dns.qry.name | sort -u > dns_q.txt
python3 -c "
import base64
with open('dns_q.txt') as f:
    for l in f:
        p=l.strip().split('.')
        if max(len(x) for x in p)>30:
            print('[TUNNEL]', l.strip())
            try: print('  Decoded:', base64.b32decode(''.join(p[:-2]).upper()+'=='))
            except: pass
"
```

### ICMP Tunneling (<30s)
```bash
FILE=$1
tshark -r $FILE -Y "icmp.type==8" -T fields -e data.data | tr -d '\n' > icmp.txt
python3 -c "
import binascii
d=open('icmp.txt').read().replace('\n','')
if d:
    try: print('[+] ICMP:', binascii.unhexlify(d))
    except: print('[+] Hex:', d[:200])
"
```

## Disk Forensics

### NTFS Alternate Data Streams & $MFT
```bash
FILE=$1
fls -r $FILE 2>/dev/null | head -40           # lists ADS (colon in name)
istat $FILE <inode>                           # inode details
mft2csv.exe -d $FILE -o mft.csv              # TZWorks
```

### File Carving
```bash
FILE=$1; OUTDIR="carved_$(basename $FILE)"; mkdir -p $OUTDIR
foremost -i $FILE -o $OUTDIR/ 2>/dev/null; binwalk -e $FILE -C $OUTDIR/ 2>/dev/null
python3 -c "
import re; data=open('$FILE','rb').read()
for ext,tag,stop in [('pdf',b'%PDF',b'%%EOF'),('png',b'\x89PNG',b'IEND'),('zip',b'PK\x03\x04',b'PK\x05\x06')]:
    for m in re.finditer(tag,data):
        s=m.start(); e=data.find(stop,s)
        if e>0: open(f'{s}_c.{ext}','wb').write(data[s:e+len(stop)]); print(f'[+] {ext}@{s}')
"
```

## Real-World CTF Examples

```
PicoCTF -- L33t St3g4n0: zsteg challenge.png -a 2>/dev/null | grep -i "flag|ctf|pico"
PicoCTF -- Packets Primer: tshark -r challenge.pcap --export-objects http,exported/
DEFCON 29 -- Bonnyr: vol3 -f mem.dmp windows.pslist | grep -i suspicious
SANS Holiday Hack 2023: ffmpeg -i goal.mp4 -vf "select=eq(n\,42)" -vframes 1 f.png; zsteg f.png -a
HTB Steganography: steghide extract -sf image.jpg -p "" 2>/dev/null
Flare-On -- Malware B64: python3 -c "import base64,re; data=open('sample.bin','rb').read(); [print('[B64]',base64.b64decode(m.group()).decode()) for m in re.finditer(b'[A-Za-z0-9+/]{20,}={0,2}',data) if base64.b64decode(m.group()).isprintable()]"
```

## Pattern Quick Solves

```
Metadata → Check Comment, GPS, IPTC/XMP        (PicoCTF Stego-100)
LSB → zsteg PNG/BMP, stegsolve bit planes       (PicoCTF m00nwalk)
Carving → binwalk -e, foremost, magic bytes      (SANS Holiday Hack 2022)
Network → Export HTTP/SMB, DNS tunnel            (DEFCON CTF Quals)
Ransomware → .locked ext, shadow copy del        (SANS FOR508)
C2 → DGA domains, base64 POST                   (Flare-On)
```

## Cloud/Container Forensics

```bash
docker cp CONTAINER:/path ./extracted; docker history IMAGE
docker save IMAGE | tar -xf - --to-stdout | strings | grep -i 'flag|password'
kubectl get secrets -A; kubectl logs POD_NAME --previous
curl -s http://169.254.169.254/latest/meta-data/
```

## Speed Metrics

```
Basic steg: <3min  |  PCAP: <5min  |  Memory: <10min  |  DNS tunnel: <5min  |  MFT: <5min
```

---

# Deep Technique Library

# CTF Forensics & Blockchain

Quick reference for forensics CTF challenges. Each technique has a one-liner here; see supporting files for full details.

## Prerequisites

**Python packages (all platforms):**
```bash
pip install volatility3 Pillow numpy matplotlib
```

**Linux (apt):**
```bash
apt install binwalk foremost libimage-exiftool-perl tshark sleuthkit \
  ffmpeg steghide testdisk john pcapfix
```

**macOS (Homebrew):**
```bash
brew install binwalk exiftool wireshark sleuthkit ffmpeg \
  testdisk john-jumbo
```

**Ruby gems (all platforms):**
```bash
gem install zsteg
```

## Additional Resources

- `forensics/3d-printing` - 3D printing forensics (PrusaSlicer binary G-code, QOIF, heatshrink)
- `forensics/windows` - Windows forensics (registry, SAM, event logs, recycle bin, NTFS alternate data streams, USN journal, PowerShell history, Defender MPLog, WMI persistence, Amcache)
- `forensics/network` - Network forensics basics (tcpdump, TLS/SSL keylog decryption, TLS master key extraction from coredump, Wireshark, PCAP, port scanning, SMB3 decryption, 5G/NR protocols, WordPress recon, credentials, USB HID steno, BCD encoding, HTTP file upload exfiltration, split archive reassembly via timestamp ordering)
- `forensics/network-advanced` - Advanced network forensics (packet interval timing encoding, NTLMv2 hash cracking, TCP flag covert channel, DNS last-byte steganography, DNS trailing byte binary encoding, multi-layer PCAP with XOR + ZIP and mDNS key, Brotli decompression bomb seam analysis, SMB RID recycling via LSARPC, Timeroasting MS-SNTP hash extraction, dnscat2 reassembly, RADIUS shared secret cracking, RC4 stream identification, ICMP payload byte rotation, ICMP ping time-delay covert channel)
- `forensics/peripheral-capture` - USB/HID/Bluetooth peripheral traffic reconstruction (USB HID mouse/pen drawing recovery, USB HID keyboard capture decoding, USB keyboard LED Morse code exfiltration, USB HID keyboard arrow key navigation tracking, Bluetooth RFCOMM packet reassembly)
- `forensics/disk-and-memory` - Core disk/memory forensics (Volatility, disk mounting/carving, VM/OVA/VMDK, VMware snapshots, GIMP raw memory dump visual inspection, coredumps, Windows KAPE triage, PowerShell ransomware, Android forensics, Docker container forensics, cloud storage forensics, BSON reconstruction, TrueCrypt/VeraCrypt mounting)
- `forensics/disk-advanced` - Advanced disk and memory techniques (deleted partitions, ZFS forensics, GPT GUID encoding, VMDK sparse parsing, memory dump string carving, ransomware key recovery, WordPerfect macro XOR, minidump ISO 9660 recovery, APFS snapshot recovery, RAID 5 XOR recovery, HFS+ resource fork recovery, Kyoto Cabinet hash DB forensics, SQLite edit history reconstruction)
- `forensics/disk-recovery` - Disk recovery and extraction patterns (LUKS master key recovery, PRNG timestamp seed brute-force, VBA macro binary recovery, FemtoZip decompression, XFS filesystem reconstruction, tar duplicate entry extraction, nested matryoshka filesystem extraction, anti-carving via null byte interleaving, BTRFS subvolume/snapshot recovery, FAT16 free space data recovery, FAT16 deleted file recovery via Sleuth Kit fls/icat, ext2 orphaned inode recovery via fsck, corrupted ZIP header repair)
- `forensics/steganography` - General steganography (binary border stego, PDF multi-layer stego, SVG keyframes, PNG reorder, file overlays, GIF frame diff Morse code, GZSteg + spammimic, spreadsheet frequency recovery, Kitty terminal graphics protocol decoding, ANSI escape sequence steganography, autostereogram solving, two-layer byte+line interleaving, multi-stream video container stego, progressive PNG layered XOR decryption, QR code reconstruction from curved reflection)
- `forensics/stego-image` - Image-specific steganography (JPEG unused DQT table LSB, BMP bitplane QR extraction, image puzzle reassembly, F5 JPEG DCT ratio detection, PNG unused palette entry stego, QR code tile reconstruction, seed-based pixel permutation + multi-bitplane QR, JPEG thumbnail pixel-to-text mapping, conditional LSB with pixel filtering, JPEG slack space, nearest-neighbor interpolation stego, RGB parity steganography)
- `forensics/stego-advanced` - Advanced steganography part 1: audio and signal techniques (FFT frequency domain, DTMF audio, SSTV+LSB, DotCode barcode, custom frequency dual-tone keypad, multi-track audio differential subtraction, cross-channel multi-bit LSB, audio FFT musical notes, audio metadata octal encoding, nested tar whitespace encoding, DeepSound audio stego with password cracking, audio waveform binary encoding, audio spectrogram hidden QR)
- `forensics/stego-advanced-2` - Advanced steganography part 2: video, image transform, and format-specific techniques (video frame accumulation, reversed audio, video frame averaging, JPEG XL TOC permutation steganography, Arnold's Cat Map descrambling, high-resolution SSTV custom FM demodulation, MJPEG FFD9 trailing byte stego, EXIF zlib + Stegano pixel patterns, PDF xref covert channel, ANSI escape code stego, pixel-wise ECB deduplication)
- `forensics/linux-forensics` - Linux/app forensics (log analysis, Docker image forensics, attack chains, browser credentials, Firefox history, TFTP, TLS weak RSA, USB audio, Git directory recovery, KeePass v4 cracking, Git reflog/fsck squash recovery, browser artifact analysis (Chrome/Chromium/Firefox history, cookies, downloads, local storage, session restore), corrupted git blob repair via byte brute-force, VBA macro Excel cell data to ELF binary extraction, Python in-memory source recovery via pyrasite)
- `forensics/signals-and-hardware` - Hardware signal decoding with decode code (VGA frame parsing, HDMI TMDS symbol decode, DisplayPort 8b/10b + LFSR descrambler), Voyager Golden Record audio, Saleae Logic 2 UART decode, Flipper Zero .sub files, side-channel power analysis (DPA), keyboard acoustic side-channel, CD audio disc image steganography (CIRC de-interleaving + spiral rendering), caps-lock LED Morse code from video, Linux input_event keylogger dump parsing, serial UART from WAV audio, USB MIDI Launchpad grid reconstruction

---

## When to Pivot

- If you recover an encrypted blob and the hard part becomes RSA, AES, or lattice work, switch to `/ctf-crypto`.
- If the evidence really points to malware staging, beacon config extraction, or packed samples, switch to `/ctf-malware`.
- If the artifact is a web app backup or API dump and the remaining problem is application logic, switch to `/ctf-web`.
- If the forensic evidence is really an encoding puzzle, steganography trick, or esoteric format rather than true forensics, switch to `/ctf-misc`.
- If you need to trace infrastructure, attribute actors, or investigate public records from forensic findings, switch to `/ctf-osint`.
- If the recovered artifact is a compiled binary or firmware that needs disassembly and analysis, switch to `/ctf-reverse`.

## Quick Start Commands

```bash
# File analysis
file suspicious_file
exiftool suspicious_file     # Metadata
binwalk suspicious_file      # Embedded files
strings -n 8 suspicious_file
hexdump -C suspicious_file | head  # Check magic bytes

# Disk forensics
sudo mount -o loop,ro image.dd /mnt/evidence
fls -r image.dd              # List files
photorec image.dd            # Carve deleted files

# Memory forensics (Volatility 3)
vol -f memory.dmp windows.info
vol -f memory.dmp windows.pslist
vol -f memory.dmp windows.filescan
```

See `forensics/disk-and-memory` for full Volatility plugin reference, VM forensics, and coredump analysis.

## Log Analysis

```bash
grep -iE "(flag|part|piece|fragment)" server.log     # Flag fragments
grep "FLAGPART" server.log | sed 's/.*FLAGPART: //' | uniq | tr -d '\n'  # Reconstruct
sort logfile.log | uniq -c | sort -rn | head         # Find anomalies
```

See `forensics/linux-forensics` for Linux attack chain analysis and Docker image forensics.

## Windows Event Logs (.evtx)

**Key Event IDs:**
- 1001 - Bugcheck/reboot
- 1102 - Audit log cleared
- 4720 - User account created
- 4781 - Account renamed

**RDP Session IDs (TerminalServices-LocalSessionManager):**
- 21 - Session logon succeeded
- 24 - Session disconnected
- 1149 - RDP auth succeeded (RemoteConnectionManager, has source IP)

```python
import Evtx.Evtx as evtx
with evtx.Evtx("Security.evtx") as log:
    for record in log.records():
        print(record.xml())
```

See `forensics/windows` for full event ID tables, registry analysis, SAM parsing, USN journal, and anti-forensics detection.

- **NTFS Alternate Data Streams (ADS):** Hidden data attached to files via named NTFS streams. Invisible to `dir`/Explorer. Detect with `fls -r image.dd | grep ":"`, extract with `icat`. See [windows.md](windows.md#ntfs-alternate-data-streams).

## When Logs Are Cleared

If attacker cleared event logs, use these alternative sources:
1. **USN Journal ($J)** - File operations timeline (MFT ref, timestamps, reasons)
2. **SAM registry** - Account creation from key last_modified timestamps
3. **PowerShell history** - ConsoleHost_history.txt (USN DATA_EXTEND = command timing)
4. **Defender MPLog** - Separate log with threat detections and ASR events
5. **Prefetch** - Program execution evidence
6. **User profile creation** - First login time (profile dir in USN journal)

See `forensics/windows` for detailed parsing code and anti-forensics detection checklist.

## Steganography

```bash
steghide extract -sf image.jpg -p ""
zsteg image.png              # PNG/BMP analysis
stegsolve                    # Visual analysis
```

- **Binary border stego:** Black/white pixels in 1px image border encode bits clockwise
- **FFT frequency domain:** Image data hidden in 2D FFT magnitude spectrum; try `np.fft.fft2` visualization
- **DTMF audio:** Phone tones encoding data; decode with `multimon-ng -a DTMF`
- **Multi-layer PDF:** Check hidden comments, post-EOF data, XOR with keywords, ROT18 final layer
- **SSTV + LSB:** SSTV signal may be red herring; check 2-bit LSB of audio samples with `stegolsb`
- **SVG keyframes:** Animation `keyTimes`/`values` attributes encode binary/Morse via fill color alternation
- **PNG chunk reorder:** Fix chunk order: IHDR → ancillary → IDAT (in order) → IEND
- **File overlays:** Check after IEND for appended archives with overwritten magic bytes
- **APNG frame extraction:** Animated PNG has multiple frames; extract with `apngdis` or parse `fdAT`/`fcTL` chunks. See [steganography.md](steganography.md#apng-animated-png-frame-extraction-icectf-2016).
- **PNG height/CRC manipulation:** Modify IHDR height field, brute-force until CRC matches to reveal hidden rows. See [steganography.md](steganography.md#png-heightcrc-manipulation-for-hidden-content-h4ckit-ctf-2016).
- **Pixel coordinate chain stego:** Linked-list traversal where R=data byte, G/B=next pixel coordinates. See [stego-image.md](stego-image.md#pixel-coordinate-chain-steganography-h4ckit-ctf-2016).
- **AVI frame differential:** XOR consecutive video frames to reveal hidden data in pixel differences. See [stego-image.md](stego-image.md#avi-frame-differential-pixel-steganography-h4ckit-ctf-2016).

- **Custom freq DTMF:** Non-standard dual-tone frequencies; generate spectrogram first (`ffmpeg -i audio -lavfi showspectrumpic`), map custom grid to keypad digits, decode variable-length ASCII
- **JPEG DQT LSB:** Unused quantization tables (ID 2, 3) carry LSB-encoded data; access via `Image.open().quantization` and extract bit 0 from each of 64 values
- **Multi-track audio subtraction:** Two nearly-identical audio tracks in MKV/video; `sox -m a0.wav "|sox a1.wav -p vol -1" diff.wav` cancels shared content, flag appears in spectrogram of difference signal (5-12 kHz band)
- **Packet interval timing:** Identical packets with two distinct interval values (e.g., 10ms/100ms) encode binary; filter by interface, compute inter-packet deltas, threshold to bits

See `forensics/steganography`, `forensics/stego-advanced`, and `forensics/stego-advanced-2` for full code examples and decoding workflows.

## PDF Analysis

```bash
exiftool document.pdf        # Metadata (often hides flags!)
pdftotext document.pdf -     # Extract text
strings document.pdf | grep -i flag
binwalk document.pdf         # Embedded files
```

**Advanced PDF stego (Nullcon 2026 rdctd):** Six techniques -- invisible text separators, URI annotations with escaped braces, Wiener deconvolution on blurred images, vector rectangle QR codes, compressed object streams (`mutool clean -d`), document metadata fields.

See `forensics/steganography` for full PDF steganography techniques and code.

## Disk / VM / Memory Forensics

```bash
# Disk images
sudo mount -o loop,ro image.dd /mnt/evidence
fls -r image.dd && photorec image.dd

# VM images (OVA/VMDK)
tar -xvf machine.ova
7z x disk.vmdk -oextracted "Windows/System32/config/SAM" -r

# Memory (Volatility 3)
vol -f memory.dmp windows.pslist
vol -f memory.dmp windows.cmdline
vol -f memory.dmp windows.netscan
vol -f memory.dmp windows.dumpfiles --physaddr <addr>

# String carving
strings -a -n 6 memdump.bin | grep -E "FLAG|SSH_CLIENT|SESSION_KEY"

# Coredump
gdb -c core.dump  # info registers, x/100x $rsp, find "flag"
```

See `forensics/disk-and-memory` for full Volatility plugin reference, VM forensics, and VMware snapshots. See `forensics/disk-advanced` for deleted partition recovery, ZFS forensics, and ransomware analysis.

## Windows Password Hashes

```bash
# Extract with impacket, crack with hashcat -m 1000
python -c "from impacket.examples.secretsdump import *; SAMHashes('SAM', LocalOperations('SYSTEM').getBootKey()).dump()"
```

See `forensics/windows` for SAM details and `forensics/network-advanced` for NTLMv2 cracking from PCAP.

## Bitcoin Tracing

- Use mempool.space API: `https://mempool.space/api/tx/<TXID>`
- **Peel chain:** ALWAYS follow LARGER output; round amounts indicate peels

## Uncommon File Magic Bytes

| Magic | Format | Extension | Notes |
|-------|--------|-----------|-------|
| `OggS` | Ogg container | `.ogg` | Audio/video |
| `RIFF` | RIFF container | `.wav`,`.avi` | Check subformat |
| `%PDF` | PDF | `.pdf` | Check metadata & embedded objects |
| `GCDE` | PrusaSlicer binary G-code | `.g`, `.bgcode` | See 3d-printing.md |

## Common Flag Locations

- PDF metadata fields (Author, Title, Keywords)
- Image EXIF data
- Deleted files (Recycle Bin `$R` files)
- Registry values
- Browser history
- Log file fragments
- Memory strings

## WMI Persistence Analysis

**Pattern (Backchimney):** Malware uses WMI event subscriptions for persistence (MITRE T1546.003).

```bash
python PyWMIPersistenceFinder.py OBJECTS.DATA
```

- Look for FilterToConsumerBindings with CommandLineEventConsumer
- Base64-encoded PowerShell in consumer commands
- Event filters triggered on system events (logon, timer)

See `forensics/windows` for WMI repository analysis details.

## Network Forensics Quick Reference

- **TFTP netascii:** Binary transfers corrupted; fix with `data.replace(b'\r\n', b'\n').replace(b'\r\x00', b'\r')`
- **TLS keylog decryption:** Import SSLKEYLOGFILE or RSA private key into Wireshark (Edit → Preferences → Protocols → TLS)
- **TLS weak RSA:** Extract cert, factor modulus, generate private key with `rsatool`, add to Wireshark
- **USB audio:** Extract isochronous data with `tshark -e usb.iso.data`, import as raw PCM in Audacity
- **NTLMv2 from PCAP:** Extract server challenge + NTProofStr + blob from NTLMSSP_AUTH, brute-force
- **WPA/WEP WiFi decryption:** `aircrack-ng -w wordlist capture.pcap` cracks WPA handshake; WEP cracked with enough IVs. See [network.md](network.md#wpawep-wifi-decryption-from-pcap-defcamp-ctf-2016).
- **PCAP repair:** `pcapfix -d corrupted.pcap` repairs broken PCAP headers/checksums for Wireshark loading. See [network.md](network.md#corrupted-pcap-repair-with-pcapfix-csaw-ctf-2016).
- **USB HID keyboard decoding:** Extract 8-byte HID reports from USB captures; byte 2 = keycode, byte 0 = modifiers (Shift). See [peripheral-capture.md](peripheral-capture.md#usb-hid-keyboard-capture-decoding-ekoparty-ctf-2016).
- **dnscat2 reassembly:** Decode hex/base32 subdomain labels, strip 9-byte dnscat2 header, deduplicate retransmissions, reassemble payload. See [network-advanced.md](network-advanced.md#dnscat2-traffic-reassembly-from-dns-pcap-bsidessf-2017).
- **USB keyboard LED exfiltration:** Host-to-device HID SET_REPORT packets toggle Caps Lock LED. Timing encodes Morse code. See [peripheral-capture.md](peripheral-capture.md#usb-keyboard-led-morse-code-exfiltration-bitsctf-2017).

See `forensics/network` for SMB3 decryption, credential extraction, and `forensics/linux-forensics` for full TLS/TFTP/USB workflows.

## Browser Forensics

- **Chrome/Edge:** Decrypt `Login Data` SQLite with AES-GCM using DPAPI master key
- **Firefox:** Query `places.sqlite` -- `SELECT url FROM moz_places WHERE url LIKE '%flag%'`

See `forensics/linux-forensics` for full browser credential decryption code.

## Additional Technique Quick References

- **Docker image forensics:** Config JSON preserves ALL `RUN` commands even after cleanup. `tar xf app.tar` then inspect config blob. See `forensics/linux-forensics`.
- **Linux attack chains:** Check `auth.log`, `.bash_history`, recent binaries, PCAP. See `forensics/linux-forensics`.
- **RAID 5 XOR recovery:** Two disks of a 3-disk RAID 5 → XOR byte-by-byte to recover the third: `bytes(a ^ b for a, b in zip(disk1, disk3))`. See [disk-advanced.md](disk-advanced.md#raid-5-disk-recovery-via-xor-crypto-cat).
- **GIMP raw memory dump visual inspection:** When Volatility fails, open `.dmp` in GIMP as raw RGB data at monitor width (~1920); scroll to find framebuffer screenshots of user's desktop. See [disk-and-memory.md](disk-and-memory.md#gimp-raw-memory-dump-visual-inspection-inshack-2018).
- **Kyoto Cabinet hash DB forensics:** Recover key ordering from KC hash database with zeroed keys by inserting sequential probe keys and binary-diffing to find which hash slot each overwrites. See [disk-advanced.md](disk-advanced.md#kyoto-cabinet-hash-database-forensics-via-incremental-key-insertion-asis-ctf-2018).
- **PowerShell ransomware:** Extract scripts from minidump, find AES key, decrypt SMTP attachment. See `forensics/disk-and-memory`.
- **Linux ransomware + memory dump:** If Volatility is unreliable, recover AES key via raw-memory candidate scanning and magic-byte validation; re-extract zip cleanly to avoid missing files/false negatives. See `forensics/disk-advanced`.
- **Deleted partitions:** `testdisk` or `kpartx -av`. See `forensics/disk-advanced`.
- **ZFS forensics:** Reconstruct labels, Fletcher4 checksums, PBKDF2 cracking. See `forensics/disk-advanced`.
- **BSON reconstruction:** Reassemble BSON (Binary JSON) documents from raw bytes; parse with `bson` Python library. See [disk-and-memory.md](disk-and-memory.md#bson-binary-json-format-reconstruction-icectf-2016).
- **TrueCrypt mounting:** Mount TrueCrypt/VeraCrypt volumes with known password using `veracrypt --mount` or `cryptsetup open --type tcrypt`. See [disk-and-memory.md](disk-and-memory.md#truecrypt--veracrypt-volume-mounting-grehack-ctf-2016).
- **Hardware signals:** VGA/HDMI TMDS/DisplayPort, Voyager audio, Saleae UART decode, Flipper Zero. See `forensics/signals-and-hardware`.
- **Caps-lock LED Morse from video:** Track caps-lock LED pixel across security camera frames with OpenCV; on/off durations encode Morse code (short=dot, long=dash). See [signals-and-hardware.md](signals-and-hardware.md#caps-lock-led-morse-code-extraction-from-video-stem-ctf-2018).
- **I2C protocol decoding:** Decode I2C bus captures (SDA/SCL lines) to extract data from EEPROM or sensor communications. See [signals-and-hardware.md](signals-and-hardware.md#i2c-bus-protocol-decoding-ekoparty-ctf-2016).
- **Punched card OCR:** Decode IBM-29 punch card images by mapping hole positions to characters using standard encoding grid. See [signals-and-hardware.md](signals-and-hardware.md#ibm-29-punched-card-ocr-ekoparty-ctf-2016).
- **USB HID mouse drawing:** Render relative HID movements per draw mode as bitmap; separate modes, skip pen lifts, scale 5-8x. See [peripheral-capture.md](peripheral-capture.md#usb-hid-mousepen-drawing-recovery-ehax-2026).
- **Side-channel power analysis:** Multi-dimensional power traces (positions × guesses × traces × samples). Average across traces, find sample with max variance, select guess with max power at leak point. See `forensics/signals-and-hardware`.
- **Packet interval timing:** Binary data encoded as inter-packet delays in PCAP. Two interval values = two bit values. See `forensics/network-advanced`.
- **BMP bitplane QR:** Extract bitplanes 0-2 per RGB channel with NumPy; hidden QR often in bit 1 (not bit 0). See [stego-image.md](stego-image.md#bmp-bitplane-qr-code-extraction--steghide-bypass-ctf-2025).
- **Image puzzle reassembly:** Edge-match pixel differences between piece borders, greedy placement in grid. See [stego-image.md](stego-image.md#image-jigsaw-puzzle-reassembly-via-edge-matching-bypass-ctf-2025).
- **DeepSound audio stego with password cracking:** Extract hash with `deepsound2john.py`, crack with John, retrieve hidden files from WAV; always check both spectrogram and DeepSound. See [stego-advanced.md](stego-advanced.md#deepsound-audio-steganography-with-password-cracking-inshack-2018).
- **QR code reconstruction from curved reflection:** Manually reconstruct QR from glass sphere reflection in video; flip, de-warp, use known plaintext prefix to fix early bytes, high ECC corrects the rest. See [steganography.md](steganography.md#qr-code-reconstruction-from-curved-glass-reflection-in-video-plaidctf-2018).
- **Audio FFT notes:** Dominant frequencies → musical note names (A-G) spell words. See `forensics/stego-advanced`.
- **Audio metadata octal:** Exiftool comment with underscore-separated octal numbers → decode to ASCII/base64. See `forensics/stego-advanced`.
- **G-code visualization:** Side projections (XZ/YZ) reveal text. See `forensics/3d-printing`.
- **Git directory recovery:** `gitdumper.sh` for exposed `.git` dirs. See `forensics/linux-forensics`.
- **KeePass v4 cracking:** Standard `keepass2john` lacks v4/Argon2 support; use `ivanmrsulja/keepass2john` fork or `keepass4brute`. Generate wordlists with `cewl`. See `forensics/linux-forensics`.
- **Cross-channel multi-bit LSB:** Different bit positions per RGB channel (R[0], G[1], B[2]) encode hidden data. See `forensics/stego-advanced`.
- **F5 JPEG DCT detection:** Ratio of ±1 to ±2 AC coefficients drops from ~3:1 to ~1:1 with F5; sparse images need secondary ±2/±3 metric. See [stego-image.md](stego-image.md#f5-jpeg-dct-coefficient-ratio-detection-apoorvctf-2026).
- **PNG unused palette stego:** Unused PLTE entries (not referenced by pixels) carry hidden data in red channel values. See [stego-image.md](stego-image.md#png-unused-palette-entry-steganography-apoorvctf-2026).
- **Keyboard acoustic side-channel:** MFCC features from keystroke audio + KNN classification against labeled reference. 10ms window captures impact transient. See `forensics/signals-and-hardware`.
- **TCP flag covert channel:** 6 TCP flag bits (FIN/SYN/RST/PSH/ACK/URG) = values 0-63, encoding base64 characters. Nonsensical flag combos on a consistent dest port = covert data. See `forensics/network-advanced`.
- **Brotli decompression bomb seam:** Compressed bomb has repeating blocks; flag breaks the pattern at a seam. Compare adjacent blocks to find discontinuity, decompress only that region. See `forensics/network-advanced`.
- **Git reflog/fsck squash recovery:** `git rebase --squash` leaves orphaned objects recoverable via `git fsck --unreachable --no-reflogs`. See `forensics/linux-forensics`.
- **DNS trailing byte binary:** Extra bytes (`0x30`/`0x31`) appended after DNS question structure encode binary bits; 8-bit MSB-first chunks → ASCII. See `forensics/network-advanced`.
- **Fake TLS + mDNS key + printability merge:** TCP stream disguised as TLS hides ZIP; XOR key from mDNS TXT record; merge two decrypted arrays by selecting printable characters. See `forensics/network-advanced`.
- **Seed-based pixel permutation stego:** Deterministic pixel shuffle (Fisher-Yates with known seed) + multi-bitplane interleaved LSB extraction from Y channel → hidden QR code. See [stego-image.md](stego-image.md#seed-based-pixel-permutation--multi-bitplane-qr-l3m0nctf-2025).
- **BTRFS snapshot recovery:** Deleted files persist in BTRFS snapshots/alternate subvolumes. `mount -o subvol=@backup` accesses historical copies. See [disk-recovery.md](disk-recovery.md#btrfs-subvolumesnapshot-recovery-bsidessf-2026).
- **JPEG XL TOC permutation:** JXL's progressive TOC permutation controls tile convergence order during partial decode. Truncate at increasing offsets, measure which tiles converge first → convergence order encodes flag. See [stego-advanced-2.md](stego-advanced-2.md#jpeg-xl-toc-permutation-steganography-bsidessf-2026).
- **Kitty terminal graphics:** `ESC_G` protocol embeds zlib-compressed RGB image data in base64 chunks. Strip escape sequences, concatenate, decompress, reconstruct. See [steganography.md](steganography.md#kitty-terminal-graphics-protocol-decoding-bsidessf-2026).
- **ANSI escape sequence stego:** Flag text interleaved between ANSI color codes and braille characters. Invisible when rendered; extract by stripping escape sequences and non-ASCII. See [steganography.md](steganography.md#ansi-escape-sequence-steganography-in-terminal-art-bsidessf-2026).
- **Autostereogram solving:** Duplicate layer, difference blend, shift horizontally ~100px to reveal hidden 3D text. See [steganography.md](steganography.md#autostereogram--magic-eye-solving-bsidessf-2026).
- **Two-layer byte+line interleaving:** Two files byte-interleaved, then scanlines interleaved. Deinterleave even/odd bytes first (valid images), then even/odd lines. See [steganography.md](steganography.md#two-layer-byteline-interleaving-bsidessf-2026).
- **SMB RID recycling:** Guest auth + LSARPC `LsaLookupSids` with incrementing RIDs enumerates AD accounts from PCAP. See [network-advanced.md](network-advanced.md#smb-rid-recycling-via-lsarpc-midnight-2026).
- **Timeroasting (MS-SNTP):** NTP requests with machine RIDs extract HMAC-MD5 hashes from DC; crack with hashcat -m 31300. See [network-advanced.md](network-advanced.md#timeroasting--ms-sntp-hash-extraction-midnight-2026).
- **Android forensics:** Extract APK with `adb pull`, analyze with `apktool`, check `shared_prefs/` and SQLite databases in `/data/data/<package>/`. See [disk-and-memory.md](disk-and-memory.md#android-forensics).
- **Docker container forensics:** `docker save` exports layered tars; deleted files persist in earlier layers. `docker history --no-trunc` reveals build secrets. See [disk-and-memory.md](disk-and-memory.md#container-forensics-docker).
- **Cloud storage forensics:** S3/GCP/Azure versioning preserves deleted objects. `list-object-versions` recovers deleted flags. See [disk-and-memory.md](disk-and-memory.md#cloud-storage-forensics-aws-s3--gcp--azure).
- **APFS snapshot recovery:** Copy-on-write filesystem preserves historical file states in snapshots; use `icat` with different XID block offsets to read inodes across transaction IDs. See [disk-advanced.md](disk-advanced.md#apfs-snapshot-historical-file-recovery-srdnlenctf-2026).
- **Windows KAPE triage:** Pre-collected artifact ZIPs; start with PowerShell history → Amcache → MFT → registry hives. See [disk-and-memory.md](disk-and-memory.md#windows-kape-triage-analysis-utctf-2026).
- **WordPerfect macro XOR:** `.wcm` files contain macros with embedded encrypted data; XOR formula `(a+b)-2*(a&b)` = bitwise XOR. See [disk-advanced.md](disk-advanced.md#wordperfect-macro-xor-extraction-srdnlenctf-2026).
- **TLS master key from coredump:** Search coredump for session ID (from Wireshark handshake); read 48 bytes before it as master key. Create Wireshark pre-master-secret log file. See [network.md](network.md#tls-master-key-extraction-from-coredump-plaidctf-2014).
- **Corrupted git blob repair:** Single-byte corruption changes SHA-1; brute-force each byte position (256 × file_size) verifying with `git hash-object`. See [linux-forensics.md](linux-forensics.md#corrupted-git-blob-repair-via-byte-brute-force-csaw-ctf-2015).
- **Split archive reassembly from PCAP:** Same-sized HTTP-transferred files with MD5-hash names are archive fragments; order by Apache directory listing timestamps, concatenate, extract password from TCP chat stream. See [network.md](network.md#split-archive-reassembly-from-http-transfers-asis-ctf-finals-2013).
- **Video frame accumulation:** Video with flashing images at various positions; composite all frames (per-pixel maximum) reveals hidden QR code or image. See [stego-advanced-2.md](stego-advanced-2.md#video-frame-accumulation-for-hidden-image-asis-ctf-finals-2013).
- **Reversed audio:** Garbled audio that sounds like speech played backwards; `sox audio.wav reversed.wav reverse` or Audacity Effect → Reverse reveals hidden message. See [stego-advanced-2.md](stego-advanced-2.md#reversed-audio-hidden-message-asis-ctf-finals-2013).
- **Multi-stream video container stego:** MP4/MKV with multiple video streams; default stream is a red herring, flag in secondary stream. `ffprobe -hide_banner file.mp4` to enumerate, `ffmpeg -i file.mp4 -map 0:1 -frames:v 1 flag.jpg` to extract. See [steganography.md](steganography.md#multi-stream-video-container-steganography-bsidessf-2026).
- **FAT16 free space recovery:** Flag hidden in unallocated clusters of FAT16 filesystem. Parse FAT table, enumerate free clusters (entry = 0x0000), read data region. See [disk-recovery.md](disk-recovery.md#fat16-free-space-data-recovery-bsidessf-2026).
- **FAT16 deleted file recovery (fls/icat):** FAT deletion replaces first byte of directory entry with `0xE5` but data remains. `fls -r -d image.img` lists deleted entries, `icat image.img <inode>` recovers by inode. See [disk-recovery.md](disk-recovery.md#fat16-deleted-file-recovery-via-sleuth-kit-metactf-flash-2026).
- **Ext2 orphaned inode recovery:** Deleted file leaves orphaned inode; `e2fsck -y disk.img` reconnects to `/lost+found`. Also use `debugfs` `lsdel` or `icat`. See [disk-recovery.md](disk-recovery.md#ext2-orphaned-inode-recovery-via-fsck-bsidessf-2026).
- **Linux input_event keylogger parsing:** 24-byte `struct input_event` binary dump; filter `type==1` (EV_KEY), `value==1` (press), map keycodes via `input-event-codes.h`. See [signals-and-hardware.md](signals-and-hardware.md#linux-input_event-keylogger-dump-parsing-pwn2win-2016).
- **VBA macro cell data to binary:** Excel cells with numeric values; VBA `CByte((val-78)/3)` transforms to ELF bytes. Reimplement in Python, never run the macro. See [linux-forensics.md](linux-forensics.md#vba-macro-forensics---excel-cell-data-to-elf-binary-sharif-ctf-2016).
- **RGB parity steganography:** Sum R+G+B per pixel; even=white, odd=black renders hidden binary bitmap. See [stego-image.md](stego-image.md#rgb-parity-steganography-break-in-2016).
- **Hidden PDF objects:** Unreferenced content stream objects not in `/Kids` array. Add to `/Kids`, increment `/Count`, re-render. See [network-advanced.md](network-advanced.md#unreferenced-pdf-objects-with-hidden-pages-sharifctf-7-2016).
- **Arnold's Cat Map descrambling:** Periodic chaotic transform on square images; iterate forward map until original reappears. Period divides `3*N`. See [stego-advanced-2.md](stego-advanced-2.md#arnolds-cat-map-image-descrambling-nuit-du-hack-2017).
- **Python in-memory source recovery:** Attach `pyrasite-shell` to running Python process, decompile `func_code` objects with `uncompyle6` (Python <=3.8) or `pycdc` (Python 3.9+), dump `globals()` for secrets. See [linux-forensics.md](linux-forensics.md#python-in-memory-source-recovery-via-pyrasite-insomnihack-2017).
- **HFS+ resource fork recovery:** Hidden data in HFS+ Resource Forks invisible to `binwalk`/`foremost`; use HFSExplorer + 010 Editor HFS template to extract extent records. See [disk-advanced.md](disk-advanced.md#hfs-resource-fork-hidden-binary-recovery-confidence-ctf-2017).
- **Serial UART from WAV audio:** Square wave in audio encodes UART serial data; determine baud rate, parse start/stop bits, decode LSB-first byte frames. See [signals-and-hardware.md](signals-and-hardware.md#serial-uart-data-decoding-from-wav-audio-easyctf-2017).
- **High-resolution SSTV demodulation:** Standard SSTV decoders fail on high-sample-rate recordings; use manual FM demodulation via `arccos` + differentiation. See [stego-advanced-2.md](stego-advanced-2.md#high-resolution-sstv-custom-fm-demodulation-plaidctf-2017).
- **Corrupted ZIP header repair:** Fix filename length fields in both Local File Header (offset 26) and Central Directory (offset 28); fallback: brute-force raw deflate at candidate offsets. See [disk-recovery.md](disk-recovery.md#corrupted-zip-repair-via-header-field-manipulation-plaidctf-2017).
- **SQLite edit history reconstruction:** Replay insert/remove diffs from SQLite diff table to reconstruct document at every intermediate state; flag may have been typed then deleted. See [disk-advanced.md](disk-advanced.md#sqlite-edit-history-reconstruction-from-diff-table-google-ctf-2017).
- **MJPEG FFD9 trailing byte stego:** Extra bytes after JPEG EOI marker (FFD9) in MJPEG frames create invisible covert channel; split on FFD8, extract post-FFD9 data. See [stego-advanced-2.md](stego-advanced-2.md#mjpeg-extra-bytes-after-ffd9-steganography-polictf-2017).
- **USB MIDI Launchpad grid reconstruction:** MIDI Note On/Off in USB PCAP maps to 8x8 Launchpad grid (`key = row*16 + col`); reconstruct visual patterns from button press sequences. See [signals-and-hardware.md](signals-and-hardware.md#usb-midi-launchpad-traffic-reconstruction-sthack-2017).

## SMB RID Recycling via LSARPC (Midnight 2026)

Enumerate AD accounts from PCAP by analyzing LSARPC `LsaLookupSids` calls with sequential RIDs after Guest auth. Filter: `dcerpc.cn_bind_to_str contains lsarpc`.

See [network-advanced.md](network-advanced.md#smb-rid-recycling-via-lsarpc-midnight-2026) for full RPC call sequence and Wireshark filters.

## Timeroasting / MS-SNTP Hash Extraction (Midnight 2026)

Extract crackable HMAC-MD5 hashes from MS-SNTP responses by sending NTP requests with machine account RIDs. Crack with `hashcat -m 31300`.

```bash
# Extract NTP payloads, convert to hashcat format, crack
tshark -r capture.pcapng -Y "ntp && ip.src == <DC_IP>" -T fields -e udp.payload
hashcat -m 31300 -a 0 -O hashes.txt rockyou.txt --username
```

See [network-advanced.md](network-advanced.md#timeroasting--ms-sntp-hash-extraction-midnight-2026) for payload parsing script and full attack chain.

## HTTP Exfiltration in PCAP

**Quick path:** `tshark --export-objects http,/tmp/objects` extracts uploaded files instantly. Check for multipart POST uploads, unusual User-Agent strings, and exfiltrated files (images with flag text). See [network.md](network.md#http-file-upload-exfiltration-in-pcap-metactf-2026).

## Common Encodings

```bash
echo "base64string" | base64 -d
echo "hexstring" | xxd -r -p
# ROT13: tr 'A-Za-z' 'N-ZA-Mn-za-m'
```

**ROT18:** ROT13 on letters + ROT5 on digits. Common final layer in multi-stage forensics. See `forensics/linux-forensics` for implementation.

## Deep Technique Files (skill_manage reference)

Load with "skill_manage read, name="ctf", reference="forensics/<file>""

- `forensics/3d-printing` — # CTF Forensics - 3D Printing / CAD File Forensics
- `forensics/disk-advanced` — # CTF Forensics - Advanced Disk and Memory Techniques
- `forensics/disk-and-memory` — # CTF Forensics - Disk and Memory Analysis
- `forensics/disk-recovery` — # CTF Forensics - Disk Recovery and Extraction Patterns
- `forensics/linux-forensics` — # CTF Forensics - Linux and Application Forensics
- `forensics/network-advanced` — # CTF Forensics - Network (Advanced)
- `forensics/network` — # CTF Forensics - Network
- `forensics/peripheral-capture` — # CTF Forensics - Peripheral Capture Analysis
- `forensics/signals-and-hardware` — # CTF Forensics - Signals and Hardware
- `forensics/steganography` — # CTF Forensics - Steganography
- `forensics/stego-advanced-2` — # CTF Forensics - Advanced Steganography (Part 2)
- `forensics/stego-advanced` — # CTF Forensics - Advanced Steganography
- `forensics/stego-image` — # CTF Forensics - Image Steganography
- `forensics/windows` — # CTF Forensics - Windows
