# CTF DFIR Skill (Digital Forensics & Incident Response)

## Speed-First Approach: Solve DFIR challenges in <15 minutes

### Phase 1: Rapid Triage (<2 minutes)
```bash
FILE=$1

# Classify artifact type
file $FILE
ls -la $FILE

# Quick flag search
strings $FILE | grep -iE 'flag\{|ctf\{|CTF\{|FLAG\{'

# Identify artifact class:
# .dmp/.raw → Memory dump (Volatility)
# .evtx → Windows Event Logs (EvtxECmd)
# .pcap/.pcapng → Network capture (tshark/Wireshark)
# .E01/.dd/.raw → Disk image (Autopsy/FTK)
# .reg → Registry hive (Registry Explorer)
# .pf → Prefetch (PECmd)
# .lnk → LNK file (LECmd)
# .sqlite → Browser/app data (DB Browser)
# .pst/.ost → Email (Thunderbird/Outlook)
```

### Phase 2: Tool Selection (<1 minute)
```
MATCH artifact to workflow:
├── Memory dump → Volatility 3, MemProcFS
├── Event logs (.evtx) → EvtxECmd → Timeline Explorer
├── MFT → MFTECmd → Timeline Explorer
├── Prefetch → PECmd
├── Registry → Registry Explorer / RegRipper
├── LNK files → LECmd
├── Amcache → AmcacheParser
├── ShellBags → ShellBags Explorer
├── Browser SQLite → DB Browser for SQLite
├── PCAP → tshark + NetworkMiner + Zui(Brim)
├── Disk image → Autopsy / FTK Imager
├── Email → Thunderbird / Outlook Forensics Wizard
└── Mobile → ALEAPP (Android) / CLEAPP (iOS)
```

### Phase 3: Solve and Submit
```
IF flag found → submit immediately
ELSE → try next tool (come back later if stuck)
```

## Memory Forensics (Volatility 3)

### Essential Volatility Workflow
```bash
FILE=$1

# Identify profile (Volatility 3 uses auto-detection)
vol -f $FILE windows.info

# Process analysis
vol -f $FILE windows.pslist        # All processes
vol -f $FILE windows.pstree        # Process tree (parent-child)
vol -f $FILE windows.psscan        # Hidden/unlinked processes
vol -f $FILE windows.cmdline       # Command-line arguments
vol -f $FILE windows.consoles      # Console history

# Network analysis
vol -f $FILE windows.netscan       # Network connections
vol -f $FILE windows.netstat       # Network statistics

# DLL analysis
vol -f $FILE windows.dlllist       # Loaded DLLs per process
vol -f $FILE windows.dlllist --pid <PID>  # DLLs for specific process

# Handle analysis
vol -f $FILE windows.handles       # Open handles per process
vol -f $FILE windows.handles --pid <PID>  # Handles for specific process

# Memory dump of process
vol -f $FILE windows.memmap --pid <PID> --dump  # Dump process memory

# Malware detection
vol -f $FILE windows.malfind       # Detect injected code, hooks
vol -f $FILE windows.vadinfo       # VAD (Virtual Address Descriptor) info

# Registry analysis
vol -f $FILE windows.registry.hivelist      # List registry hives
vol -f $FILE windows.registry.printkey      # Print registry key
vol -f $FILE windows.registry.userassist    # UserAssist entries (program execution)
vol -f $FILE windows.registry.amcache       # Amcache (installed programs)
vol -f $FILE windows.registry.shimcache     # AppCompatCache (program execution)

# File analysis
vol -f $FILE windows.filescan      # Scan for file objects
vol -f $FILE windows.dumpfiles --virtaddr <ADDRESS>  # Dump file by address

# Credential extraction
vol -f $FILE windows.hashdump      # SAM hash dump
vol -f $FILE windows.lsadump       # LSA secrets dump

# Timeline
vol -f $FILE timeliner            # Full timeline of activities
```

### MemProcFS (Mount Memory as Filesystem)
```bash
FILE=$1
MOUNTPOINT=/tmp/memprocfs

# Mount memory dump as filesystem
memprocfs -device $FILE -mount $MOUNTPOINT

# Explore processes as directories
ls $MOUNTPOINT/pid_*/
cat $MOUNTPOINT/pid_*/cmdline
cat $MOUNTPOINT/pid_*/environ

# Extract process memory
cat $MOUNTPOINT/pid_*/proc/*.bin > extracted.bin

# Network connections
cat $MOUNTPOINT/pid_*/net/*

# Unmount when done
fusermount -u $MOUNTPOINT
```

### Memory Forensics Patterns from Real Cases
```
Pattern: Process Injection (malfind)
- Suspicious: Unknown memory regions with RWX permissions
- Detection: Volatility malfind, check for PE headers in non-image sections
- Key indicators: Unbacked executable memory, APC queue injection

Pattern: Credential Dumping
- Suspicious: lsass.exe accessed by unusual process
- Detection: Volatility handles on lsass.exe, check for MiniDump
- Key indicators: procdump, comsvcs.dll, Task Manager dump

Pattern: Fileless Malware
- Suspicious: PowerShell/Script process with encoded commands
- Detection: Volatility cmdline, consoles for decoded commands
- Key indicators: Base64 encoded PS commands, WMI persistence

Pattern: Rootkit Detection
- Hidden processes visible in psscan but not pslist
- SSDT hooks, IRP hooking
- Check kernel modules: windows.modules, windows.driverscan
```

## Windows Event Log Analysis (EvtxECmd)

### Essential Event Log Workflow
```bash
FILE=$1

# Parse all event logs
EvtxECmd.exe -f $FILE --csv output/ --csvf timeline.csv

# Or parse specific log
EvtxECmd.exe -f Security.evtx --csv output/ --csvf security.csv

# Key Event IDs to search:
# 4624 - Successful logon
# 4625 - Failed logon (brute force detection)
# 4648 - Explicit credentials (RunAs, PsExec)
# 4672 - Special privileges assigned (admin logon)
# 4688 - Process creation (with command-line)
# 4697 - Service installation
# 4698 - Scheduled task creation
# 4720 - User account created
# 4732 - Member added to local group
# 5140 - Network share accessed
# 5156 - Windows Filtering Platform connection
# 7045 - Service installation (System log)
# 1102 - Audit log cleared (anti-forensics)
```

### Timeline Explorer Workflow
```bash
# Generate super timeline
EvtxECmd.exe -f "C:\Windows\System32\winevt\Logs\*.evtx" --csv output/ --csvf full_timeline.csv

# Open in Timeline Explorer
# Filter by:
# - Date range (attack window)
# - Event ID (specific activity)
# - Source (specific log)
# - Message content (keywords)
```

### Event Log Quick Analysis
```bash
# Search for brute force
grep -i "4625" security.evtx.log | awk -F',' '{print $5}' | sort | uniq -c | sort -rn

# Search for suspicious process creation
grep -i "4688" security.evtx.log | grep -iE "powershell|cmd\.exe|wscript|cscript|mshta|regsvr32"

# Search for service installation
grep -i "7045" system.evtx.log

# Search for cleared logs
grep -i "1102" security.evtx.log

# Search for scheduled tasks
grep -i "4698" security.evtx.log
```

### Key Artifact Locations (Windows)
```
C:\Windows\System32\winevt\Logs\     # Event logs
C:\Windows\System32\config\           # Registry hives
C:\Windows\Prefetch\                  # Prefetch files
C:\Windows\Temp\                      # Temp files
C:\Users\<user>\AppData\Local\        # User-specific data
C:\Users\<user>\NTUSER.DAT            # User registry hive
C:\$MFT                               # Master File Table
C:\$LogFile                           # NTFS journal
C:\hiberfil.sys                       # Hibernation file
C:\pagefile.sys                       # Page file
C:\swapfile.sys                       # Swap file
```

## MFT Analysis (MFTECmd)

```bash
FILE=$1

# Parse MFT
MFTECmd.exe -f $FILE --csv output/ --csvf mft_timeline.csv

# Key MFT entries to check:
# - $STANDARD_INFORMATION: Creation, modification, access times
# - $FILE_NAME: Original filenames (harder to modify)
# - File content (resident vs non-resident)

# Check for timestomping
# Compare $STANDARD_INFORMATION vs $FILE_NAME timestamps
# Significant differences = likely anti-forensic manipulation
```

## Prefetch Analysis (PECmd)

```bash
FILE=$1

# Parse prefetch file
PECmd.exe -f $FILE

# Key information:
# - Executable name and path
# - Run count (how many times executed)
# - Last run time
# - Volume references (which drives)

# Parse all prefetch files
PECmd.exe -d "C:\Windows\Prefetch" --csv output/ --csvf prefetch.csv
```

## Registry Analysis

### Registry Explorer
```bash
# Load hive in Registry Explorer
# Key hives to analyze:
# NTUSER.DAT - User-specific settings
# SAM - User accounts and hashes
# SECURITY - Security policies
# SOFTWARE - Installed software
# SYSTEM - System configuration

# Key registry keys for forensics:
# HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run     # Autostart
# HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run     # User autostart
# HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce # One-time autostart
# HKLM\SYSTEM\CurrentControlSet\Services                   # Services
# HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Prefetcher  # Prefetch
# HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\RecentDocs  # Recent docs
# HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\RunMRU  # Run dialog history
```

### RegRipper Quick Analysis
```bash
FILE=$1

# Parse SYSTEM hive
rip.pl -r $FILE -p system

# Parse SOFTWARE hive
rip.pl -r $FILE -p software

# Parse NTUSER.DAT
rip.pl -r $FILE -p ntuser

# Parse SAM
rip.pl -r $FILE -p sam
```

## Browser Forensics

### SQLite Database Analysis
```bash
# Chrome/Edge history
DB="C:\Users\<user>\AppData\Local\Google\Chrome\User Data\Default\History"
sqlite3 "$DB" "SELECT url, title, visit_count, last_visit_time FROM urls ORDER BY last_visit_time DESC;"

# Chrome/Edge cookies
sqlite3 "$DB" "SELECT name, value, host_key, path FROM cookies;"

# Chrome/Edge logins
sqlite3 "$DB" "SELECT origin_url, username_value, date_created FROM logins;"

# Firefox history
DB="C:\Users\<user>\AppData\Roaming\Mozilla\Firefox\Profiles\<profile>\places.sqlite"
sqlite3 "$DB" "SELECT url, title, visit_count, last_visit_date FROM moz_places;"
```

### Browsing History View (NirSoft)
```bash
# Run BrowsingHistoryView.exe
# Export to CSV for analysis
# Check for:
# - Downloads of suspicious files
# - Visits to malicious domains
# - Credential-related pages
# - Time correlation with attack timeline
```

## Email Forensics

### PST/OST Analysis
```bash
# Use Outlook Forensics Wizard or Thunderbird
# Key artifacts:
# - Email headers (Originating IP, SPF/DKIM)
# - Attachments (malicious Office docs, LNK files)
# - Send/receive timestamps
# - Deleted items (recoverable)

# Header analysis:
# Check for spoofed sender addresses
# Verify SPF/DKIM/DMARC
# Trace originating IP via Received: headers
```

## File Carving & Recovery

### Volume Shadow Copy Extraction
```bash
# Use vssadmin or FTK Imager
vssadmin list shadows                    # List shadow copies
copy \\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy1\Windows\System32\config\SAM SAM.bak

# Extract deleted files from shadow copies
# Often contains files that were "deleted" but exist in shadow copies
```

### Recycle Bin Analysis
```bash
# Use rifiuti2
rifiuti2.exe "C:\$Recycle.Bin" --csv output.csv

# Key information:
# - Original file path
# - Deletion timestamp
# - File size
# - Whether file is still recoverable
```

## DFIR Automation Scripts

### Full Artifact Extraction
```bash
#!/bin/bash
# dfir_extract.sh - Extract all forensic artifacts
SOURCE=$1
OUTDIR="dfir_$(date +%Y%m%d_%H%M%S)"
mkdir -p $OUTDIR

# Event logs
cp -r "$SOURCE/Windows/System32/winevt/Logs/" $OUTDIR/eventlogs/ 2>/dev/null

# Registry hives
cp "$SOURCE/Windows/System32/config/SAM" $OUTDIR/ 2>/dev/null
cp "$SOURCE/Windows/System32/config/SECURITY" $OUTDIR/ 2>/dev/null
cp "$SOURCE/Windows/System32/config/SOFTWARE" $OUTDIR/ 2>/dev/null
cp "$SOURCE/Windows/System32/config/SYSTEM" $OUTDIR/ 2>/dev/null

# User registry
find "$SOURCE/Users" -name "NTUSER.DAT" -exec cp {} $OUTDIR/ \; 2>/dev/null

# Prefetch
cp -r "$SOURCE/Windows/Prefetch/" $OUTDIR/prefetch/ 2>/dev/null

# Browser artifacts
find "$SOURCE/Users" -path "*/AppData/Local/Google/Chrome/User Data/Default/History" -exec cp {} $OUTDIR/chrome_history.sqlite \; 2>/dev/null

# MFT
cp "$SOURCE/$MFT" $OUTDIR/ 2>/dev/null

echo "Extraction complete. Check $OUTDIR/"
ls -la $OUTDIR/
```

### Automated Event Log Triage
```bash
#!/bin/bash
# eventlog_triage.sh - Quick event log analysis
LOGDIR=$1

echo "=== Brute Force Detection (4625) ==="
grep -l "4625" $LOGDIR/*.evtx.log 2>/dev/null | while read f; do
  echo "--- $f ---"
  grep "4625" "$f" | awk -F',' '{print $5}' | sort | uniq -c | sort -rn | head -5
done

echo "=== Suspicious Process Creation (4688) ==="
grep -h "4688" $LOGDIR/*.evtx.log 2>/dev/null | grep -iE "powershell|cmd\.exe|wscript|cscript|mshta|regsvr32|rundll32|certutil" | head -20

echo "=== Service Installation (7045) ==="
grep -h "7045" $LOGDIR/*.evtx.log 2>/dev/null | head -10

echo "=== Log Cleared (1102) ==="
grep -h "1102" $LOGDIR/*.evtx.log 2>/dev/null

echo "=== Scheduled Tasks (4698) ==="
grep -h "4698" $LOGDIR/*.evtx.log 2>/dev/null | head -10
```

## Common DFIR Patterns from Real Cases

### Ransomware Investigation
```
1. Check event logs for initial access:
   - 4624/4625 (logon events)
   - 4688 (process creation - look for phishing attachment)
   - 1102 (log clearing - often done by ransomware)

2. Check for shadow copy deletion:
   - vssadmin delete shadows
   - wmic shadowcopy delete
   - bcdedit /set {default} recoveryenabled no

3. Check for lateral movement:
   - 5140 (network share access)
   - 4648 (explicit credentials)
   - 7045 (PsExec service installation)

4. Check for encryption markers:
   - File extensions changed
   - Ransom note files
   - Mass file modifications in timeline
```

### Malware Infection Timeline
```
1. Initial Access:
   - Email attachment opened (Event 4688: WINWORD.EXE → cmd.exe)
   - Downloaded file executed (Event 4688: browser → PowerShell)
   - Exploited application (Event 4688: app crash, suspicious child)

2. Execution:
   - Macro execution (Event 4688: WINWORD.EXE → PowerShell)
   - Script execution (Event 4688: wscript/cscript/mshta)
   - Living-off-the-land (Event 4688: certutil, mshta, regsvr32)

3. Persistence:
   - Registry run keys (Registry: HKCU\...\Run)
   - Scheduled tasks (Event 4698)
   - Services (Event 7045)
   - Startup folder shortcuts

4. C2 Communication:
   - DNS queries to unusual domains
   - HTTP/HTTPS to unfamiliar IPs
   - Encoded/obfuscated URLs
```

### Credential Theft Detection
```
1. LSASS Access:
   - Check handles on lsass.exe (Volatility)
   - Check for credential dumping tools in prefetch
   - Check event logs for suspicious process access

2. Kerberoasting:
   - Event 4769: Service Ticket Requests (encryption type 0x17)
   - Multiple TGS requests from same source

3. Pass-the-Hash:
   - Event 4624: Logon Type 3 with NTLM
   - Event 4648: Explicit credential logon
```

## Speed Metrics
```
Average solve times (target):
- Memory dump process listing: <3 minutes
- Event log triage: <5 minutes
- Timeline construction: <10 minutes
- Browser forensics: <5 minutes
- Registry analysis: <5 minutes
- Full DFIR investigation: <15 minutes
```
