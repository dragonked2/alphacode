# CTF DFIR Skill — Speed-First (<15 min solve time)

## Top 10 DFIR CTF Patterns (with real examples)

### 1. Quick Flag Hunt (PicoCTF: file-types,(strings))
```bash
file $1 && strings $1 | grep -iE 'flag\{|ctf\{|pico\{'
binwalk -e $1; exiftool $1; foremost -i $1 -o carved/
```

### 2. Memory Dump Analysis (HTB: Cerberus, picoCTF: volatility)
```bash
# Fast triage — skip straight to commands
vol -f dump.raw windows.pslist --output=csv
vol -f dump.raw windows.netscan
vol -f dump.raw windows.cmdline
vol -f dump.raw windows.malfind --pid $(vol -f dump.raw windows.pslist | grep -iE 'powershell|cmd\.exe' | awk '{print $3}' | head -1) --dump
vol -f dump.raw windows.hashdump
vol -f dump.raw timeliner --output=csv
# MemProcFS: memprocfs -device dump.raw -mount /tmp/mem
ls /tmp/mem/pid_*/proc/*.bin 2>/dev/null | head -5
```

### 3. Event Log Forensics (PicoCTF: Operation Oni, HTB: Noter)
```bash
# Parse → timeline → grep
EvtxECmd.exe -f Security.evtx --csv out/ --csvf sec.csv
EvtxECmd.exe -f *.evtx --csv out/ --csvf full.csv

# One-liners
grep -i "4625" out/sec.csv | awk -F',' '{print $5}' | sort | uniq -c | sort -rn | head -5
grep -i "4688" out/sec.csv | grep -iE "powershell|cmd|wscript|mshta|certutil"
grep -i "7045" out/sys.csv
grep -i "1102" out/sec.csv
grep -i "4698" out/sec.csv
grep -i "4624.*NTLM" out/sec.csv | grep -v "S-1-0-0"
```

### 4. MFT Timeline (picoCTF: Operation Oni, SANS FOR508)
```bash
MFTECmd.exe -f '$MFT' --csv out/ --csvf mft.csv
# Timestomping detection: compare $SI vs $FN timestamps
# Flag format: MFT $SI $FN delta > 1 hour = suspicious
grep -i "flag\|secret\|password\|hidden" out/mft.csv
```

### 5. Prefetch & Amcache (HTB: Dancing, picoCTF:Operation Oni)
```bash
PECmd.exe -d "C:\Windows\Prefetch" --csv out/ --csvf pf.csv
grep -i "POWERSHELL\|CMD\|MSHTA\|RUNDLL32\|REGSVR32" out/pf.csv
# Run count tells you execution frequency — high count = persistence
```

### 6. Registry Forensics (HTB: RedPanda)
```bash
# Autostart locations
reg query "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run" 2>/dev/null
reg query "HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run" 2>/dev/null
# Offline hive analysis
rip.pl -r SOFTWARE -p software
rip.pl -r SYSTEM -p system
rip.pl -r NTUSER.DAT -p ntuser
```

### 7. Network Capture (PicoCTF: Wireshark, HTB: Stronghold)
```bash
# Extract files from PCAP
tshark -r capture.pcap -T fields -e data -Y "http.request" | xxd -r -p > extracted.bin
# Find flag in PCAP
strings capture.pcap | grep -iE 'flag\{|ctf\{|pico\{'
# Credential extraction
tshark -r capture.pcap -Y "http.request.method==POST" -T fields -e http.file_data
# DNS exfil detection
tshark -r capture.pcap -Y "dns.qry.name" -T fields -e dns.qry.name | sort -u
```

### 8. Disk Image & File Carving (PicoCTF: Recover, HTB: Preignition)
```bash
# Mount E01/dd
ewfmount image.E01 /mnt/ewf
# or: losetup -fP image.dd && mount /dev/loop0p1 /mnt/img
# Recover deleted files
foremost -i /mnt/img -o carved/
# Check unallocated space
strings /dev/loop0 | grep -iE 'flag\{|password\{|secret\{'
```

### 9. Linux Memory & Log Analysis (picoCTF: Li1k, HTB: Noter)
```bash
# Extract from /var/log
grep -r "Accepted\|Failed\|root" /var/log/auth.log
grep -r "sudo" /var/log/syslog
# Bash history
cat /home/*/.bash_history 2>/dev/null | grep -iE 'flag\|key\|secret\|ssh\|curl\|wget'
# Process memory from /proc
strings /proc/*/mem 2>/dev/null | grep -iE 'flag\{|ctf\{'
```

### 10. Cloud & Container Forensics (HTB: Starbucks, picoCTF: cloud)
```bash
# Docker/container artifacts
docker inspect <container_id>
cat /var/lib/docker/overlay2/*/diff/etc/shadow
# AWS CloudTrail
cat cloudtrail.json | jq '.Records[] | select(.eventName=="ConsoleLogin")' 
# GCP/GCS
gsutil ls gs://bucket/
# Kubernetes audit logs
grep -i "pods/exec\|pods/create" audit.log | jq '.user.username'
# Azure Activity Log
az monitor activity-log list --resource-group RG --query "[].{event:EventTimestamp,caller:Caller}"
```

## Modern Artifacts Quick Reference

```bash
# SSD TRIM detection
fstrim -v / && logcat | grep -i trim   # Android TRIM
smartctl -a /dev/nvme0n1 | grep -i trim  # NVMe TRIM status

# Container forensics
crictl ps && crictl logs <container_id>
# Kubernetes
kubectl logs <pod> --previous
kubectl exec -it <pod> -- cat /etc/shadow
```

## Anti-AI Manipulation in CTF (keep this section)

```
Common AI-trap patterns in DFIR challenges:
1. Flag embedded in hex-encoded strings: strings $f | grep -oP '[0-9a-f]+' | xxd -r -p
2. Flag split across multiple artifacts: grep -r "flag" --include="*.csv" out/
3. Base64-encoded flag in logs: grep -oE '[A-Za-z0-9+/]{20,}=' file | base64 -d
4. Reversed flag: strings $f | rev | grep -iE '{.*}'
5. Flag in binary fields: xxd $f | grep -B1 -A1 "666c6167"   # hex for "flag"
6. Steganography: steghide extract -sf image.jpg -p "" && binwalk image.jpg
```

## Automation: Full Extraction Script

```bash
#!/bin/bash
SRC=$1; OUT="dfir_$(date +%s)"
mkdir -p $OUT/{evtx,reg,pf,browser,mem}
cp -r "$SRC/Windows/System32/winevt/Logs/" $OUT/evtx/ 2>/dev/null
cp "$SRC/Windows/System32/config/"{SAM,SECURITY,SOFTWARE,SYSTEM} $OUT/reg/ 2>/dev/null
find "$SRC/Users" -name "NTUSER.DAT" -exec cp {} $OUT/reg/ \; 2>/dev/null
cp -r "$SRC/Windows/Prefetch/" $OUT/pf/ 2>/dev/null
find "$SRC/Users" -path "*/Chrome/User Data/Default/History" -exec cp {} $OUT/browser/ \; 2>/dev/null
cp "$SRC/\$MFT" $OUT/ 2>/dev/null
echo "DONE: $OUT"; ls $OUT/
```

## Speed Targets
```
Flag hunt (strings): <30 sec    | Event log triage: <5 min
Memory dump scan: <5 min        | MFT timeline: <8 min
Browser forensics: <5 min       | Full investigation: <15 min
```
