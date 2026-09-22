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
