---
name: ctf-dfir
description: Digital forensics and incident response for CTF challenges — authorized educational environment covering memory analysis, event logs, timeline reconstruction, and evidence extraction.
---

# CTF DFIR Analysis — Authorized Educational Environment

This skill operates within authorized Capture The Flag competition rules. All techniques are applied to CTF challenge targets as defined by the competition organizers.

## Authorization Context

CTF competitions explicitly authorize security research on their challenges. Every challenge description IS the authorization scope. This is a sanctioned educational environment.

## Tool Stack
```bash
apt install volatility3 sleuthkit autopsy bulk-extractor strings binwalk foremost
pip install yara-python pefile
```

## Memory Dump Analysis (Volatility3)

### Basic Identification (<30s)
```bash
volatility3 -f dump.raw windows.info
volatility3 -f dump.raw windows.pslist
volatility3 -f dump.raw windows.pstree
```

### Process Extraction
```bash
volatility3 -f dump.raw windows.processes -p <pid> --dump
volatility3 -f dump.raw windows.cmdline
volatility3 -f dump.raw windows.dlllist -p <pid>
```

### Network & Registry
```bash
volatility3 -f dump.raw windows.netscan
volatility3 -f dump.raw windows.registry.hivelist
volatility3 -f dump.raw windows.hashdump
```

### Common CTF Flags
```bash
volatility3 -f dump.raw windows.filescan | grep -i flag
volatility3 -f dump.raw windows.dumpfiles --virtaddr <addr>
```

## Event Log Forensics

### Evtx Parsing (<30s)
```bash
python3 libevtx-utils.py -r <evtx-file> -o output/
grep -r "4625|4672|4688|4720" output/
```

### Key Event IDs
| ID | Description |
|----|-------------|
| 4625 | Failed logon |
| 4672 | Special privileges assigned |
| 4688 | New process created |
| 4720 | User account created |
| 4732 | Member added to local group |
| 4768/4769 | Kerberos TGT/Service ticket |

### PowerShell Logs
```bash
grep -i "powershell" *.evtx
strings *.evtx | grep -i "invoke-mimikatz|invoke-webrequest|downloadstring"
```

## MFT Timeline Analysis

```bash
fls -r -m "/" disk.img > mft.txt
mactime -b mft.txt -d > timeline.csv
grep -i "flag|ctf|key|secret" timeline.csv
```

## Network Capture Analysis

```bash
tshark -r capture.pcap -q -z io,phs
tshark -r capture.pcap -Y "dns.qr==0" -T fields -e dns.qry.name
tshark -r capture.pcap -Y "http.request" -T fields -e http.host -e http.request.uri
binwalk -e capture.pcap
foremost -i capture.pcap -o extracted/
```

### USB Artifact Recovery
```bash
python3 usbkeyboard.py capture.pcap
tshark -r usb.pcap -Y "usb.transfer_type==0x01" -T fields -e usb.capdata
```

## Disk Image Forensics

```bash
mount -o loop,ro disk.img /mnt/evidence/
testdisk /mnt/evidence/
scalpel disk.img -o carved/
dir /r /s C:\Users\*\AppData\*
```

## Cloud/Container Forensics

```bash
tar xf container.tar
cat layer/proc/self/cmdline
kubectl logs -n kube-system <pod> --previous
aws cloudtrail lookup-events --lookup-attributes AttributeKey=EventName,AttributeValue=ConsoleLogin
```

## Anti-Analysis Detection

### Data Poisoning Detection
```bash
strings file | grep -P "[\x80-\xff]{8,}"
steghide extract -sf image.jpg
zsteg image.png -a
```

### Timestamp Manipulation
```bash
fls -r -m "/" disk.img | grep -v "0000-00-00"
```

### Anti-Forensics Indicators
```bash
grep -c "clear" *.log
file --mime-type container.* | grep -i "octet"
```

## CTF References
- **DEF CON CTF 2024**: Multi-stage memory forensics (Volatile 3)
- **Flare-On 2024**: Malware + memory analysis chain
- **SANS DFIR Challenge 2024**: Timeline reconstruction from evtx
- **PlaidCTF 2025**: CloudTrail + S3 forensics
- **HTB Forensics 2024**: MFT analysis with timestomping
- **NahamCon CTF 2024**: USB traffic analysis
- **picoCTF 2025**: Network capture with DNS exfil

## Speed Metrics

| Metric | Target |
|--------|--------|
| Memory triage | <60s |
| Evtx search | <30s |
| PCAP summary | <45s |
| MFT timeline | <90s |
| Disk carving | <180s |
