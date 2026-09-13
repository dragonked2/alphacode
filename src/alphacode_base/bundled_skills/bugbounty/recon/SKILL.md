---
name: recon
description: Attack surface discovery — subdomains, DNS, tech detection, directory fuzzing, JS analysis, inventory building.
---

# RECON — 3 BULLETS MAX

**Core:** Recon done when you know exactly what to test and in what order.

## PASSIVE
```bash
curl -s "https://crt.sh/?q=%.target.com&output=json" | jq -r '.[].name_value' | sort -u > ct.txt
subfinder -d target.com -all -o subfinder.txt
cat ct.txt subfinder.txt | sort -u > subs.txt
```

## ACTIVE
```bash
dnsx -l subs.txt -a -aaaa -cname -resp -o resolved.txt
httpx -l resolved.txt -sc -title -tech-detect -cdn -follow-redirects -o alive.txt
ffuf -u https://target.com/FUZZ -w common.txt -o fuzz.json
katana -u target.com -d 3 -jc | grep -oE "/api/[a-zA-Z0-9/_-]+" | sort -u > apis.txt
```

## JS ANALYSIS
```bash
curl -s "https://target.com/" | grep -oE 'src="[^"]*\.js"' | while read js; do
  curl -s "$js" | grep -oiE "(api[_-]?key|secret|token|password)['\"]?\s*[:=]\s*['\"][^'\"]+['\"]"
done
```

## INVENTORY OUTPUT
```
Endpoint | Auth | State-Changing | Priority
/api/v1/users | Yes | Yes | HIGH (IDOR)
/api/checkout | Yes | Yes | CRITICAL (payment)
/graphql | Yes | Yes | HIGH (rich surface)
/admin | Yes(admin) | Yes | CRITICAL (privilege)
/auth/reset | No | Yes | HIGH (password reset)
```
