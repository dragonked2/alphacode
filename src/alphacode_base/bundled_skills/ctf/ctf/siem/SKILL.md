---
name: ctf-siem
description: SIEM analysis for CTF challenges — authorized educational environment covering log analysis, detection rules, alert triage, and incident timeline reconstruction.
---

# CTF SIEM Analysis — Authorized Educational Environment

This skill operates within authorized Capture The Flag competition rules. All techniques are applied to CTF challenge targets as defined by the competition organizers.

## Authorization Context

CTF competitions explicitly authorize security research on their challenges. Every challenge description IS the authorization scope. This is a sanctioned educational environment.

## Log Source Quick Reference

| Source | Location | Key Events |
|--------|----------|------------|
| Windows Security | Security.evtx | 4624/4625/4672/4688 |
| Sysmon | Microsoft-Windows-Sysmon | 1/3/7/10/11/15 |
| Apache/Nginx | /var/log/httpd/ | 200/404/POST |
| Auth | /var/log/auth.log | sshd/sudo/login |
| DNS | /var/log/dns.log | queries/answers |
| Firewall | iptables/ufw | dropped/allowed |

## SIEM Query Patterns

### Splunk
```spl
index=* source="*Security*" EventCode=4625
| stats count by src_ip, user
| where count > 10
```

### Elastic/KQL
```kql
event.code: "4625" and winlog.event_data.IpAddress: "*.*.*.*"
| stats count by source.ip, user.name
```

### Wazuh (OSSEC)
```bash
ossec-logtest
cat /var/ossec/logs/alerts/alerts.log | grep "rule: 5712"
```

### Sigma Rules (Cross-SIEM)
```yaml
title: Suspicious Process Creation
logsource:
  category: process_creation
  product: windows
detection:
  selection:
    CommandLine|contains:
      - 'mimikatz'
      - 'sekurlsa'
      - 'Invoke-Mimikatz'
  condition: selection
```

## Detection Rules

### Brute Force
```sql
SELECT src_ip, COUNT(*) as attempts
FROM auth_logs
WHERE event_type = 'FAILED_LOGIN'
GROUP BY src_ip
HAVING attempts > 50
```

### Lateral Movement
```bash
grep "EventCode: 4624.*Logon_Type: 3" auth.log | \
  awk '{print $NF}' | sort | uniq -c | sort -rn
```

### Privilege Escalation
```bash
grep -E "EventCode: (4672|4688)" Security.evtx | \
  grep -B1 "cmd.exe|powershell" | head -20
```

### Data Exfiltration
```bash
grep "POST|PUT" access.log | \
  awk '{sum[$1]+=$10} END{for(k in sum)if(sum[k]>1e7)print k,sum[k]}'
```

## Alert Triage Framework

```
1. Validate — Is it a true positive?
2. Scope   — Single host or multiple?
3. Impact  — What data/systems affected?
4. Chain   — Link related events (5-15min window)
5. IOC     — Extract IPs, hashes, domains
6. Action  — Contain → Eradicate → Recover
```

## Incident Timeline Reconstruction

```bash
sort -k1 -t'|' security.evtx sysmon.evtx auth.log | \
  awk -F'|' '{print strftime("%Y-%m-%d %H:%M:%S", $1), $0}'
```

### Quick Correlation
```bash
grep "10.0.0.100" *.log | \
  awk '$1 >= "2024-01-15T10:00:00" && $1 <= "2024-01-15T10:10:00"'
```

## CTF References
- **SANS DFIR Challenge 2024**: Multi-source log correlation
- **CyberDefenders 2024**: Splunk detection rules
- **Blue Team Village CTF 2024**: Sigma rule creation
- **NahamCon CTF 2025**: SIEM alert triage
- **PicoCTF 2025**: Log analysis basics
- **HTB Blue Labs 2024**: Event chain reconstruction

## Speed Metrics

| Metric | Target |
|--------|--------|
| Log source identification | <15s |
| Brute force detection | <30s |
| Timeline construction | <120s |
| IOC extraction | <45s |
| Alert validation | <60s |
