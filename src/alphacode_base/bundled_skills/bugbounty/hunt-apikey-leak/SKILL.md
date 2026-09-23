---
name: hunt-apikey-leak
description: API key/secret detection — JS bundles, source code, config files, error messages, backup files. Must demonstrate sensitive capability impact.
---

# API KEY LEAK HUNTING — 3 BULLETS MAX

**Core:** Exposed keys are only vulnerabilities if they provide sensitive capability.

## DETECTION
```bash
# JS bundles
katana -u target.com -d 3 -jc | grep "\.js$" | xargs -I{} curl -s {} | grep -oiE "(api[_-]?key|secret|token|password|authorization|bearer|oauth|jwt|private[_-]?key)['\"]?\s*[:=]\s*['\"][^'\"]+['\"]"
# Config files
for f in .env config.yml config.json docker-compose.yml .git/config .htaccess web.config; do
  curl -s "https://target.com/$f" | head -5
done
# Source code leaks
curl -s "https://target.com/.git/HEAD"  # Git repo exposed
curl -s "https://target.com/.env" | grep -iE "key|secret|token|password"
# Error messages
curl -s "https://target.com/api/error" | grep -iE "key|token|secret|password"
# Backup files
for f in backup.zip source.zip db.sql database.sql dump.sql; do
  code=$(curl -s -o /dev/null -w "%{http_code}" "https://target.com/$f")
  [ "$code" != "404" ] && echo "[+] $f → $code"
done
```

## IMPACT ASSESSMENT
```
AWS key → test: aws sts get-caller-identity → Critical if admin
Stripe key → test: create $0.10 charge → Critical if production
SendGrid key → test: send email → High
GitHub token → test: list repos → High if private repos
Database URL → connect to DB → Critical
JWT secret → forge tokens → Critical
OAuth client_secret → impersonate app → High
```

## CHAINS
```
API key → cloud access → infrastructure compromise → Critical
OAuth secret → impersonate app → ATO → Critical
JWT secret → forge admin tokens → Critical
Database URL → mass data exfil → Critical
```
