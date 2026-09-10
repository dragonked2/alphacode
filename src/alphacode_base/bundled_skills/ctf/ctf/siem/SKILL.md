# CTF SIEM Analysis Skill

## Speed-First Approach: Solve SIEM challenges in <10 minutes

### Phase 1: Rapid Triage (<2 minutes)
```bash
FILE=$1

# Classify log type
file $FILE
head -20 $FILE

# Quick pattern recognition:
# JSON format → ELK/Elasticsearch
# Key-value pairs → Splunk
# XML/CEF → Wazuh/Syslog
# Windows Event Log → EvtxECmd + Timeline Explorer
# CSV/TSV → Generic log analysis

# Quick flag search
grep -iE 'flag\{|ctf\{|CTF\{|FLAG\{' $FILE

# Quick IOC search
grep -iE 'suspicious|malicious|alert|warning|critical|error' $FILE | head -20
```

### Phase 2: Tool Selection (<1 minute)
```
MATCH log to tool:
├── JSON logs → jq, kibana queries
├── Windows Event Logs → EvtxECmd, deepbluecli
├── Syslog → grep, awk, splunk queries
├── CSV → awk, python pandas, Excel
├── PCAP logs → tshark
└── Custom format → Python script
```

### Phase 3: Solve and Submit
```
IF flag found → submit immediately
ELSE → try next tool (come back later if stuck)
```

## Essential SIEM Query Patterns

### Splunk Queries
```spl
# Failed logon attempts (brute force)
index=windows EventCode=4625 | stats count by src_ip, target_user | where count > 5

# Successful logon after failed attempts
index=windows EventCode=4624 | join src_ip [search index=windows EventCode=4625 | stats count by src_ip | where count > 3] | table _time, src_ip, target_user, logon_type

# Process creation with suspicious parents
index=windows EventCode=4688 | search parent_process_name IN ("winword.exe", "excel.exe", "powershell.exe") | table _time, process_name, command_line, parent_process_name

# Service installation
index=windows EventCode=7045 | table _time, service_name, service_type, start_type, image_path

# Scheduled task creation
index=windows EventCode=4698 | table _time, task_name, task_content

# Log cleared
index=windows EventCode=1102 | table _time, target_user, src_ip

# Network connections to external IPs
index=network dest_ip!=10.0.0.0/8 dest_ip!=192.168.0.0/16 dest_ip!=172.16.0.0/12 | stats count by src_ip, dest_ip, dest_port | where count > 10

# DNS queries to suspicious domains
index=dns | search query NOT IN ("*.microsoft.com", "*.google.com", "*.windows.com") | stats count by query | where count > 5

# Data exfiltration (large transfers)
index=network | stats sum(bytes_out) as total_bytes by src_ip | where total_bytes > 1000000000 | sort -total_bytes
```

### ELK/Kibana Queries
```json
// Failed logons (brute force)
{
  "query": {
    "bool": {
      "must": [
        { "match": { "event.code": "4625" } }
      ]
    }
  },
  "aggs": {
    "by_source": {
      "terms": { "field": "source.ip", "size": 20 }
    }
  }
}

// Suspicious process creation
{
  "query": {
    "bool": {
      "must": [
        { "match": { "event.code": "4688" } },
        { "wildcard": { "process.name": ["powershell.exe", "cmd.exe", "wscript.exe", "mshta.exe"] } }
      ]
    }
  }
}

// Lateral movement (PsExec)
{
  "query": {
    "bool": {
      "must": [
        { "match": { "event.code": "7045" } },
        { "wildcard": { "service.name": "PSEXE*" } }
      ]
    }
  }
}
```

### Linux Log Analysis
```bash
# Brute force detection
grep "Failed password" /var/log/auth.log | awk '{print $11}' | sort | uniq -c | sort -rn | head -10

# Successful logins after failures
grep "Accepted" /var/log/auth.log | awk '{print $11}' | sort | uniq -c | sort -rn

# Suspicious cron jobs
cat /var/log/syslog | grep -i cron | grep -v "CMD (run-parts)"

# Web server access anomalies
awk '{print $1}' /var/log/apache2/access.log | sort | uniq -c | sort -rn | head -20

# Command execution
grep "COMMAND" /var/log/auth.log | tail -20

# Sudo usage
grep "sudo:" /var/log/auth.log | tail -20
```

### Windows Event ID Quick Reference
```
=== AUTHENTICATION ===
4624 - Successful logon
4625 - Failed logon
4634 - Logoff
4647 - User initiated logoff
4648 - Explicit credential logon (RunAs, PsExec)
4672 - Special privileges assigned (admin)

=== PROCESS ===
4688 - Process creation (with command-line)
4689 - Process termination
5156 - Windows Filtering Platform connection

=== SERVICES ===
7045 - Service installation
7036 - Service started/stopped
7040 - Service start type changed

=== SCHEDULED TASKS ===
4698 - Scheduled task created
4699 - Scheduled task deleted
4700 - Scheduled task enabled
4702 - Scheduled task updated

=== ACCOUNT MANAGEMENT ===
4720 - User account created
4722 - User account enabled
4725 - User account disabled
4726 - User account deleted
4728 - Member added to security-enabled global group
4732 - Member added to local group

=== POLICY CHANGES ===
4719 - Audit policy changed
4739 - Domain policy changed

=== ANTI-FORENSICS ===
1102 - Audit log cleared

=== OBJECT ACCESS ===
4663 - Attempt to access object
5140 - Network share accessed
5145 - Network share object access
```

## Common SIEM Challenge Patterns

### Pattern: Brute Force Attack
```
Detection:
- Multiple 4625 events from same source IP
- Multiple failed logons for same account
- Short time window between attempts

Investigation:
1. Count failed logons by source IP
2. Check if any succeeded (4624) after failures
3. Identify target accounts
4. Check for lateral movement after successful logon
```

### Pattern: Lateral Movement
```
Detection:
- 4648 (explicit credentials) from unusual sources
- 7045 (service installation) - PsExec
- 4698 (scheduled task) on remote hosts
- 5140 (network share) access from non-admin accounts

Investigation:
1. Map source → destination connections
2. Check service names and paths
3. Identify execution context
4. Trace credential usage
```

### Pattern: Data Exfiltration
```
Detection:
- Large outbound data transfers
- DNS queries to unusual domains (DNS tunneling)
- HTTP/HTTPS to unfamiliar IPs
- Off-hours large transfers

Investigation:
1. Identify top talkers by data volume
2. Check destination IPs against threat intel
3. Analyze DNS query patterns
4. Review HTTP request content
```

### Pattern: Privilege Escalation
```
Detection:
- 4672 (special privileges) for non-admin accounts
- 4728/4732 (group membership changes)
- 4719 (audit policy changed)
- 4739 (domain policy changed)

Investigation:
1. Identify account that gained privileges
2. Check how privileges were gained
3. Review group membership changes
4. Check for persistent access mechanisms
```

### Pattern: Defense Evasion
```
Detection:
- 1102 (log clearing)
- Missing expected event logs
- Gaps in timeline
- Service stopping events before suspicious activity

Investigation:
1. Check for log gaps
2. Identify which logs were cleared
3. Look for backup/secondary logging
4. Correlate with other data sources
```

## Automated SIEM Analysis Scripts

### Log Triage Script
```bash
#!/bin/bash
# log_triage.sh - Quick SIEM log analysis
LOGFILE=$1

echo "=== Log Statistics ==="
wc -l $LOGFILE
head -5 $LOGFILE
echo ""

echo "=== Error/Critical/Warning Counts ==="
grep -ciE 'error|critical|warning' $LOGFILE
echo ""

echo "=== Failed Logons (4625) ==="
grep -c "4625" $LOGFILE 2>/dev/null || echo "N/A"
echo ""

echo "=== Successful Logons (4624) ==="
grep -c "4624" $LOGFILE 2>/dev/null || echo "N/A"
echo ""

echo "=== Process Creation (4688) ==="
grep -c "4688" $LOGFILE 2>/dev/null || echo "N/A"
echo ""

echo "=== Suspicious Processes ==="
grep "4688" $LOGFILE 2>/dev/null | grep -iE 'powershell|cmd\.exe|wscript|cscript|mshta|regsvr32|rundll32|certutil' | head -10
echo ""

echo "=== Service Installation (7045) ==="
grep "7045" $LOGFILE 2>/dev/null | head -5
echo ""

echo "=== Log Cleared (1102) ==="
grep "1102" $LOGFILE 2>/dev/null
echo ""

echo "=== Top Source IPs ==="
grep -oE '[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}' $LOGFILE | sort | uniq -c | sort -rn | head -10
```

### Event ID Frequency Analysis
```bash
#!/bin/bash
# event_frequency.sh - Analyze event ID distribution
LOGFILE=$1

echo "=== Event ID Frequency ==="
grep -oP 'EventID[=:]\s*\K[0-9]+' $LOGFILE | sort | uniq -c | sort -rn | head -20

echo ""
echo "=== Timeline (events per hour) ==="
grep -oP '\d{4}-\d{2}-\d{2}T\d{2}' $LOGFILE | sort | uniq -c | sort -k2
```

### IOC Search in Logs
```bash
#!/bin/bash
# ioc_search.sh - Search for IOCs in logs
LOGFILE=$1

echo "=== IP Addresses ==="
grep -oE '[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}' $LOGFILE | sort -u

echo "=== Domains ==="
grep -oE '[a-z0-9]+\.[a-z]{2,}' $LOGFILE | sort -u | head -20

echo "=== URLs ==="
grep -oE 'https?://[^ ]+' $LOGFILE | sort -u

echo "=== File Paths ==="
grep -oE '[A-Z]:\\[^ ]+' $LOGFILE | sort -u | head -20

echo "=== Registry Keys ==="
grep -oE 'HK[A-Z]+\\[^ ]+' $LOGFILE | sort -u | head -20

echo "=== Executables ==="
grep -oiE '[a-z0-9_-]+\.exe' $LOGFILE | sort -u
```

## SIEM Configuration Intelligence

### Useful Windows Log Settings to Enable
```
=== Security Log ===
- Audit Logon Events (Success, Failure)
- Audit Account Management (Success, Failure)
- Audit Privilege Use (Success)
- Audit Object Access (Success, Failure)
- Audit Policy Change (Success, Failure)
- Audit System Events (Success, Failure)

=== System Log ===
- Service installation events
- System startup/shutdown
- Driver loading

=== PowerShell Logging ===
- Module Logging (Event 4103)
- Script Block Logging (Event 4104)
- Transcription Logging

=== Sysmon (if available) ===
- Process creation (Event 1)
- Network connections (Event 3)
- File creation (Event 11)
- Registry modification (Event 13)
- WMI activity (Event 19, 20, 21)
```

### Wazuh Rules Quick Reference
```xml
<!-- Brute force detection -->
<rule id="100001" level="10">
  <if_sid>5712</if_sid>
  <frequency>5</frequency>
  <timeframe>300</timeframe>
  <description>Brute force attack detected</description>
</rule>

<!-- Suspicious process -->
<rule id="100002" level="12">
  <match>powershell.*-enc|cmd.*\/c.*whoami|certutil.*-urlcache</match>
  <description>Suspicious command execution</description>
</rule>

<!-- Log cleared -->
<rule id="100003" level="12">
  <id>1102</id>
  <description>Security log cleared - possible anti-forensics</description>
</rule>
```

## Speed Metrics
```
Average solve times (target):
- Log type identification: <1 minute
- Basic event filtering: <2 minutes
- Brute force detection: <3 minutes
- Lateral movement mapping: <5 minutes
- Timeline construction: <5 minutes
- Full SIEM investigation: <10 minutes
```
