# CTF Solution Verifier — Pre-Submission Gate

## Core Rule: Never Submit Without Verification

Wrong flags cost points, waste time, and reveal your approach. Verification time < recovery time.

## Automated Flag Format Checker

```python
#!/usr/bin/env python3
import re, sys

KNOWN = {
    'ctfd':     r'^flag\{[a-zA-Z0-9_!@#$%^&*()\-+=\[\]{}|;:\'",.<>?/\\~` ]+\}$',
    'picoctf':  r'^picoCTF\{[a-zA-Z0-9_]+\}$',
    'htb':      r'^HTB\{[a-zA-Z0-9_!@#$%^&*()\-+=]+\}$',
    'thm':      r'^THM\{[a-zA-Z0-9_]+\}$',
    'hitcon':   r'^hitcon\{[a-zA-Z0-9_]+\}$',
    'ductf':    r'^DUCTF\{[a-zA-Z0-9_!@#$%^&*()\-+=]+\}$',
    'generic':  r'^(flag|ctf|FLAG|CTF|Flag)\{[a-zA-Z0-9_!@#$%^&*()\-+=]+\}$',
}

HONEYPOT = [
    r'^(test|placeholder|dummy|fake)', r'\{0+\}|\{a+\}|\{test',
    r'(.)\1{5,}', r'^flag\{\}$', r'flag\{test', r'flag\{admin',
]

def validate(flag, expected=None):
    r = {'flag': flag, 'ok': True, 'warnings': [], 'format': None}
    if not flag or len(flag) < 5:
        return {'ok': False, 'warnings': ['Too short'], 'flag': flag}
    for name, pat in KNOWN.items():
        if re.match(pat, flag):
            r['format'] = name; break
    if expected and expected in KNOWN:
        if not re.match(KNOWN[expected], flag):
            r['warnings'].append(f'Expected {expected}'); r['ok'] = False
    elif not r['format']:
        r['warnings'].append('No format match')
    for p in HONEYPOT:
        if re.search(p, flag, re.IGNORECASE):
            r['warnings'].append(f'HONEYPOT: {p}'); r['ok'] = False
    if flag != flag.strip():
        r['warnings'].append('Whitespace')
    return r

def batch_check(flags, expected=None):
    return [validate(f.strip(), expected) for f in flags if f.strip()]

if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == '--batch':
        flags = sys.stdin.read().strip().split('\n')
        fmt = sys.argv[2] if len(sys.argv) > 2 else None
        for r in batch_check(flags, fmt):
            s = "PASS" if r['ok'] else "FAIL"
            print(f"[{s}] {r['flag']}" + (f" → {r['warnings']}" if r['warnings'] else ""))
    else:
        flag = sys.argv[1] if len(sys.argv) > 1 else input('Flag: ')
        r = validate(flag)
        print(f'OK: {r["ok"]} | Format: {r["format"]} | Warnings: {r["warnings"]}')
```

## Real Honeypot Detection Examples

```
picoCTF 2019 "strings":
- Found: flag{d3bugg3r_ad0pt3d_d0g} via `strings` → HONEYPOT (<30s)
- Real: .rodata section after XOR decode

HTB "Noter":
- Found: HTB{sql_injection_is_easy} in SQL error → HONEYPOT (describes vuln)
- Real: Admin panel backup file

THM "Internal":
- Found: THM{placeholder_flag} in robots.txt → HONEYPOT (placeholder text)
- Real: SSH authorized_keys on pivot box
```

## Metadata Cross-Reference

```
POINTS vs COMPLEXITY:
- 500+ pts → significant effort expected
- 100-200 pts → straightforward
- <100 pts → quick solve
- RED FLAG: High points + easy solution = probably trap

SOLVE COUNT:
- 0 solves → very hard or new
- 1-5 solves → non-obvious approach needed
- 20+ solves → pattern is probably simple
```

## Honeypot Detection

```
□ FOUND IN <2 MINUTES → real flags require effort
□ FOUND IN OBVIOUS LOCATION → strings, comments, README = decoys
□ MULTIPLE FLAGS → at least one honeypot; submit least obvious
□ SOLUTION FELT "TOO EASY" → easy to hard = trap
□ CATEGORY MISMATCH → crypto solved with web = probably wrong
```

## Final Submission Gate

```
FORMAT:     □ Matches CTF format exactly □ No whitespace □ Correct caps
HONEYPOT:   □ NOT in obvious location □ NOT found in <2 min □ Least obvious if multiple
LOGIC:      □ Clear chain □ Technique matches □ No luck
CONFIDENCE: □ HIGH or MEDIUM □ No warnings

IF ANY BOX UNCHECKED → DO NOT SUBMIT
```

## Post-Submission Learning

```
IF CORRECT: What made it correct? What trap did you avoid?
IF WRONG: What trap did you fall for? Add to detection DB.
```
