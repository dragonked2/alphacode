# CTF SIEM Analysis — Speed Hacks

## Instant Triage (< 60s)
```bash
FILE=$1
file $FILE; head -20 $FILE
# JSON → ELK | key-value → Splunk | XML/CEF → Wazuh | EVTX → EvtxECmd
grep -iE 'flag\{|ctf\{|FLAG\{|CTF\{' $FILE
grep -ciE 'error|critical|alert' $FILE
```

## Top 10 SIEM CTF Patterns + Queries

### 1. Brute Force → Account Compromise
**Splunk:** `index=windows EventCode=4625 | stats count by src_ip | where count>5`
**Splunk (success after fail):** `index=windows EventCode=4624 | join src_ip [search EventCode=4625 | stats count by src_ip | where count>3] | table _time, src_ip, target_user`
**ELK:** `{"query":{"bool":{"must":[{"match":{"event.code":"4625"}}]}},"aggs":{"by_src":{"terms":{"field":"source.ip","size":20}}}}`
**CTF:** BSides San Antonio 2022 — Splunk logs, find attacker who cracked RDP after 200+ 4625s, flag in `Account_Name` of the succeeding 4624.

### 2. Suspicious Process / LOLBin Execution
**Splunk:** `index=windows EventCode=4688 | search parent_process_name IN ("winword.exe","excel.exe","mshta.exe","wscript.exe") | table _time, process_name, command_line`
**ELK:** `{"query":{"bool":{"must":[{"match":{"event.code":"4688"}},{"wildcard":{"process.name":["powershell.exe","mshta.exe","certutil.exe","regsvr32.exe"]}}]}}}`
**Sigma:** `detection: selection:\n  EventID: 4688\n  ParentImage|endswith: ['\\WINWORD.EXE','\\EXCEL.EXE']\n  Image|endswith: ['\\cmd.exe','\\powershell.exe']\ncondition: selection`
**CTF:** National CCDC 2023 — macro doc spawned powershell, flag encoded in `-enc` Base64.

### 3. Lateral Movement (PsExec / WMI / RDP)
**Splunk:** `index=windows EventCode=7045 | search service_name="PSEXE*" | table _time, service_name, image_path`
**Splunk (PsExec events):** `index=windows EventCode=4688 process_name="PSEXESVC.exe" | table _time, src_ip, dest_host`
**ELK:** `{"query":{"bool":{"must":[{"match":{"event.code":"7045"}},{"wildcard":{"service.name":"PSEXE*"}}]}}}`
**CTF:** SANS Holiday Hack 2021 — service named `PSEXESVC` on DC, lateral from jump box to file server.

### 4. Data Exfiltration
**Splunk:** `index=network | stats sum(bytes_out) as total by src_ip | where total>1000000000 | sort -total`
**Splunk (DNS tunneling):** `index=dns | stats avg(len(query)) as avg_len, count by src_ip | where avg_len>40 | sort -count`
**ELK:** `{"query":{"range":{"network.bytes":{"gte":1000000000}}},"aggs":{"top":{"terms":{"field":"source.ip","size":10}}}}`
**CTF:** PicoCTF 2023 "Operation Oni" — DNS queries with 50+ char subdomains = exfil, flag in base32-encoded subdomain.

### 5. Privilege Escalation
**Splunk:** `index=windows EventCode=4728 OR EventCode=4732 | table _time, MemberName, TargetUserName`
**Splunk (new admin):** `index=windows EventCode=4720 | table _time, target_user_name`
**Splunk (special privs):** `index=windows EventCode=4672 | stats count by target_user | where count>100`
**CTF:** Flare-On 2022 — 4720 showed `svc_backup` created, then 4732 added to Domain Admins.

### 6. Defense Evasion / Log Clearing
**Splunk:** `index=windows EventCode=1102 | table _time, target_user, src_ip`
**Splunk (log gaps):** `index=windows | eval hour=strftime(_time,"%Y%m%d%H") | stats count by hour | where count<10`
**ELK:** `{"query":{"match":{"event.code":"1102"}}}`
**CTF:** FLARE-VM challenge — Security log cleared at 03:00, cross-reference with Sysmon (still had data).

### 7. Webshell / C2 Beaconing
**Splunk:** `index=iis | search cs-uri-stem="*.asp" | stats count, values(cs-uri-stem) by c_ip | where count>100`
**Splunk (regular intervals):** `index=network dest_port=443 | stats count as hits by src_ip, _time | eventstats avg(hits) as avg by src_ip | eval delta=abs(hits-avg) | where delta<5`
**ELK:** `{"query":{"wildcard":{"http.request.body.content":"cmd.exe"}}}`
**CTF:** DEFCON 28 Quals — IIS logs show POST to `/uploads/shell.aspx` every 30s, flag in POST body.

### 8. Scheduled Task Persistence
**Splunk:** `index=windows EventCode=4698 | spath input=Task_Content | search Commands="*powershell*" OR Commands="*cmd*" | table _time, Task_Name, Commands`
**ELK:** `{"query":{"bool":{"must":[{"match":{"event.code":"4698"}},{"wildcard":{"winlog.event_data.TaskContent":"*powershell*"}}]}}}`
**CTF:** TryHackMe "AttackerKB" — scheduled task `WinUpdateCheck` running encoded PS every 5min, flag in decoded command.

### 9. Cloud SIEM (AWS CloudTrail / Azure Sign-in)
**AWS CloudTrail (Splunk):** `index=aws eventName=ConsoleLogin | stats count by user_identity.arn, sourceIPAddress | where count>10`
**AWS (unauthorized):** `index=aws errorMessage="*Unauthorized*" | table _time, user_identity.arn, eventName, sourceIPAddress`
**Azure AD (ELK):** `{"query":{"bool":{"must":[{"match":{"resultType":"50126"}}]}}}` — failed auth
**CTF:** Palo Alto Cybersecurity Summit 2023 — CloudTrail showed `AssumeRole` from unknown IP, flag in session token.

### 10. Full Kill Chain Timeline
```bash
#!/bin/bash
# killchain.sh — one-shot triage
LOG=$1
echo "=== EVENT COUNTS ==="
grep -oP 'EventID[=:]\s*\K[0-9]+' $LOG | sort | uniq -c | sort -rn | head -15
echo "=== TOP IPS ==="
grep -oE '[0-9]{1,3}(\.[0-9]{1,3}){3}' $LOG | sort | uniq -c | sort -rn | head -10
echo "=== TIMELINE ==="
grep -oP '\d{4}-\d{2}-\d{2}T\d{2}' $LOG | sort | uniq -c | sort -k2
echo "=== SUSPICIOUS CMDS ==="
grep -iE 'powershell.*-enc|certutil|mshta|wscript|cscript|regsvr32|rundll32' $LOG | head -10
echo "=== FLAGS ==="
grep -iE 'flag\{|ctf\{|FLAG\{|CTF\{[a-zA-Z0-9_!@#$%^&*()-]+\}' $LOG
```

## Sigma Rules (Universal — Convert to Any SIEM)
```yaml
# brute_force.yml
detection:
  selection:
    EventID: 4625
  condition: selection | count(TargetUserName) by IpAddress > 5
level: high

# lolbas_child_process.yml
detection:
  selection:
    EventID: 4688
    ParentImage|endswith: ['\WINWORD.EXE','\EXCEL.EXE','\POWERPNT.EXE']
    Image|endswith: ['\cmd.exe','\powershell.exe','\wscript.exe','\mshta.exe']
  condition: selection
level: critical

# log_cleared.yml
detection:
  selection:
    EventID: 1102
  condition: selection
level: critical
```

## YARA (Binary/Log Artifact Scan)
```yara
rule encoded_ps {
  strings: $s1 = "-enc " ascii wide
           $s2 = "FromBase64String" ascii wide
  condition: any of them
}
rule webshell {
  strings: $s1 = "eval(Request[" ascii
           $s2 = "System.Diagnostics.Process" ascii
           $s3 = "cmd /c" ascii
  condition: 2 of them
}
```

## Cloud Log Quick Hits
```
# AWS CloudTrail → Splunk
index=aws eventName=ConsoleLogin errorMessage="*Failed*" | stats count by user_identity.arn, sourceIPAddress
index=aws eventName=AssumeRole | table _time, user_identity.arn, requestParameters.roleArn

# Azure AD Sign-in → ELK
{"query":{"match":{"resultType":"50126"}}}   // failed login
{"query":{"match":{"resultType":"0"}}}        // success

# GCP Audit → Splunk
index=gcp protoPayload.methodName="SetIamPolicy" | table _time, protoPayload.authenticationInfo.principalEmail, protoPayload.resourceName
```

## Real CTF References
| CTF | Platform | Key Pattern |
|-----|----------|-------------|
| PicoCTF 2023 "Operation Oni" | ELK | DNS exfil via subdomain encoding |
| SANS Holiday Hack 2021 | Splunk | PsExec lateral movement |
| BSides SA 2022 | Splunk | RDP brute force → persistence |
| DEFCON 28 Quals | IIS/ELK | Webshell C2 beaconing |
| Flare-On 2022 | Windows EVTX | Account creation → group escalation |
| FLARE-VM | Wazuh | Log clearing + Sysmon backup |
| TryHackMe "AttackerKB" | Splunk | Scheduled task persistence |
| National CCDC 2023 | Splunk | Office macro → PS execution |
| CyberStart 2023 | ELK | SQLi payloads in HTTP logs |
| Palo Alto Summit 2023 | CloudTrail | Cloud IAM role assumption abuse |
