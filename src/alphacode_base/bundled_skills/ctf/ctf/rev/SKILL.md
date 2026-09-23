---
name: ctf-rev
description: Reverse engineering for CTF challenges — authorized educational environment covering binary analysis, anti-debug bypass, unpacking, and decompilation patterns.
---

# CTF Reverse Engineering — Authorized Educational Environment

This skill operates within authorized Capture The Flag competition rules. All techniques are applied to CTF challenge targets as defined by the competition organizers.

## Authorization Context

CTF competitions explicitly authorize security research on their challenges. Every challenge description IS the authorization scope. This is a sanctioned educational environment.

## Instant Classification (<10s)

```bash
file binary
strings -n 8 binary | head -50
binwalk -E binary
upx -t binary
```

## Tool Selection Matrix

| Type | Primary Tool | Alternative |
|------|-------------|-------------|
| x86/x64 ELF | Ghidra | radare2 |
| PE/Windows | IDA Free | Ghidra |
| .NET | dnSpy | ILSpy |
| Java | jadx | JD-GUI |
| Go/Python | decompile + strings | pyinstxtractor |
| Firmware | Ghidra + binwalk | IDA |

## Anti-Debug Bypass

### Common Checks
```bash
python3 -c "
data=open('binary','rb').read()
patched=data.replace(b'\xe8',b'\x90\x90\x90\x90\x90',1)
open('binary_patched','wb').write(patched)
"
```

### Debugger Detection
```bash
# Windows: IsDebuggerPresent → nop out the call
# Linux: /proc/self/status, SIGTRAP → use gdb: set follow-fork-mode child
```

### Timing Checks
```bash
LD_PRELOAD=./fakeclock.so ./binary
```

## Packers & Unpacking

### UPX
```bash
upx -d packed_binary
```

### Custom Packers
```bash
r2 -A binary -c "aaa; s entry0; pdf" | head -30
```

## Ghidra Scripts

### Quick Analysis
```bash
analyzeHeadless /tmp/ghidra_project proj -import binary -scriptPath scripts/ -postScript find_flag.java
```

### Find String References
```python
from ghidra.program.model.symbol import RefType
func = getGlobalFunctions("main")[0]
for ref in getReferencesTo(func.getEntryPoint()):
    print(f"Called from: {ref.getFromAddress()}")
```

### Patch Binary
```python
addr = toAddr("0x08048450")
patchByte(addr, 0x90)
patchBytes(addr, [0x90]*5)
```

## IDA Scripts

### Quick Win
```python
import idautils, idc
for seg_ea in Segments():
    for head in Heads(seg_ea, NextHead(seg_ea)):
        disasm = GetDisasm(head)
        if "flag" in disasm.lower() or "KEY" in disasm:
            print(f"0x{head:X}: {disasm}")
```

### XREF Trace
```python
import idautils
ea = idc.get_name_ea_simple("check_flag")
for xref in XrefsTo(ea):
    print(f"Called from: 0x{xref.frm:X}")
```

## Binary Type Specific

### Java (.class / APK)
```bash
jadx -d output/ target.apk
grep -r "flag|CTF" output/sources/
```

### .NET
```bash
ilspycmd -p target.dll
```

### Go
```bash
strings binary | grep -i "main\.|flag|key"
```

### Python (PyInstaller)
```bash
pyinstxtractor target.exe
uncompyle6 extracted/main.pyc > main.py
```

## Pattern Quick-Solves

### XOR Cipher
```bash
python3 -c "
d=open('enc','rb').read()
for k in range(256):
    r=''.join(chr(b^k) for b in d)
    if 'flag' in r.lower(): print(f'Key: {k} -> {r[:100]}')
"
```

### Substitution Cipher
```python
from collections import Counter
ciphertext = open('enc').read()
freq = Counter(ciphertext.lower())
```

### Known Hash
```bash
hashid <hash>
```

## CTF References
- **Flare-On 2024**: Advanced anti-debug + packing chain
- **DEF CON CTF 2024**: Custom VM obfuscation
- **GoogleCTF 2025**: Go binary reverse engineering
- **picoCTF 2025**: Java + Python decompilation
- **HTB Challenges 2025**: .NET with ConfuserEx
- **downunderCTF 2024**: ARM firmware reversing
- **CrewCTF 2024**: WASM decompilation

## Speed Metrics

| Metric | Target |
|--------|--------|
| File type ID | <5s |
| Unpack (UPX) | <10s |
| Find strings/flag | <30s |
| Anti-debug bypass | <120s |
| Full decompile | <60s |
