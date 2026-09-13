# CTF Reverse Engineering — Speed-First

## Instant Classification (<1 min)
```bash
FILE=$1; file $FILE; strings -n8 $FILE | head -20
strings $FILE | grep -iE 'flag\{|ctf\{|SECCON|correct|password|key'
python3 -c "
import math;d=open('$FILE','rb').read();f=[d.count(bytes([i]))/len(d)for i in range(256)]
e=-sum(x*math.log2(x)for x in f if x>0);print(f'Entropy:{e:.2f}({\"Packed\"if e>7.5 else \"Normal\"})')"
strings $FILE | grep -iE 'UPX|themida|vmprotect|obsidium'
```

## Tool Selection
```
ELF→Ghidra,radare2,angr,z3 | PE→x64dbg,IDA,DIE
.NET→dnSpy,de4dot | Java→jadx,jd-gui | Python→uncompyle6,pycdc
Go→Ghidra,GoReSym | Rust→Ghidra,IDA | Android→jadx,apktool,unidbg
JavaScript→de4js | Packed→UPX -d | VM-based→Unidbg,Qiling
```

## Anti-Debug Bypass
```bash
# Detect: objdump -d $FILE | grep -iE 'ptrace|IsDebugger|NtQuery|rdtsc'
# Linux ptrace: NOP the call (replace with MOV EAX,0)
# Windows: patch IsDebuggerPresent with XOR EAX,EAX; RET (0x33C0C3)
# Timing: measure execution, patch RDTSC sequences
```

### Ghidra Anti-Debug Finder
```python
from ghidra.app.decompiler import DecompInterface
decomp=DecompInterface();decomp.openProgram(currentProgram)
for func in currentProgram.getFunctionManager().getFunctions(True):
    r=decomp.decompileFunction(func,10,monitor)
    if r.getDecompiledFunction():
        c=r.getDecompiledFunction().getC()
        for p in ['ptrace','IsDebugger','NtQuery','rdtsc','CheckRemoteDebugger']:
            if p in c:print(f"[!] {p} in {func.getName()}")
```

## Packers & Unpacking
```bash
upx -t binary && upx -d binary          # UPX unpack
die binary                               # Detect packer (DIE)
# Themida/VMP: use x64dbg+ScyllaHide, trace imports, dump after OEP
```
```python
# Qiling generic unpack
from qiling import Qiling
ql=Qiling([path],rootfs="/")
ql.hook_address(lambda q:print(f"OEP:{q.reg.arch_pc:#x}"),0x401000)
ql.run()
```

## VM-Based Obfuscation
```python
# Unidbg Android native emulation
import sys;sys.path.append("unidbg-android/src")
from unidbpy import DvmUnicornEngine
e=DvmUnicornEngine(dex_path)
e.call_method("Lcom/ctf/Challenge;","checkFlag","(Ljava/lang/String;)Z",["flag{test}"])

# ByteCode VM detection: look for computed jump (jmp [table+idx*8])
# IDAPython: find dispatch tables
import idautils,idc
for ea in idautils.Heads():
    if idc.print_insn_mnem(ea)=='jmp' and '[' in idc.print_operand(ea):
        print(f"VM dispatch: {ea:#x} {idc.GetDisasm(ea)}")
```

## Ghidra Auto-Solve Script
```python
from ghidra.app.decompiler import DecompInterface
decomp=DecompInterface();decomp.openProgram(currentProgram)
for f in currentProgram.getFunctionManager().getFunctions(True):
    if f.getName() in ['main','WinMain','_start']:
        r=decomp.decompileFunction(f,30,monitor)
        if r.getDecompiledFunction():
            c=r.getDecompiledFunction().getC()
            if any(x in c for x in ['strcmp','memcmp','flag','xor']):
                print(f"=== {f.getName()} ===\n{c[:2000]}");break
```

## IDA String Decode
```python
import idautils,idc,ida_bytes
for seg in idautils.Segments():
    for ea in idautils.Heads(idc.get_segm_start(seg),idc.get_segm_end(seg)):
        d=ida_bytes.get_bytes(ea,16)
        if d and len(d)>=8:
            for k in range(1,256):
                dec=bytes([b^k for b in d])
                if b'flag' in dec or b'FLAG' in dec:
                    print(f"XOR {k:#x}@{ea:#x}:{dec.decode(errors='ignore')}")
```

## Deobfuscation
```python
# Control flow flattening detection (Ghidra)
import re
for f in currentProgram.getFunctionManager().getFunctions(True):
    r=decomp.decompileFunction(f,10,monitor)
    if r.getDecompiledFunction():
        c=r.getDecompiledFunction().getC()
        if c.count('goto')>10 and 'switch' in c:
            print(f"[!] Flattened: {f.getName()}")

# Char array XOR decode (from source/binary)
import re
for m in re.finditer(r'0x([0-9a-f]{2})',source):
    # Collect consecutive hex values, XOR with key
    pass  # implement per-challenge pattern
```

## Pattern Quick Solves

### XOR Brute (Flare-On style)
```python
data=open('binary','rb').read()
for k in range(256):
    r=bytes([b^k for b in data[0x100:0x200]])
    if b'flag' in r.lower()or b'flare' in r.lower():print(f'Key:{k:#x}→{r}')
```

### strcmp / memcmp (SECCON style)
```bash
objdump -d $FILE | grep -B3 -A3 'strcmp'
strings -tx $FILE | grep -iE 'flag|correct|success'
```

### Math per-char
```python
import operator;data=open('binary','rb').read()
for k in range(256):
    for op,n in[(operator.add,'ADD'),(operator.sub,'SUB'),(operator.xor,'XOR')]:
        t=bytes([op(b,k)&0xFF for b in data[:5]])
        if t.isascii()and t[:4]==b'flag':print(f'{n} k={k:#x}:{bytes([op(b,k)&0xFF for b in data[:50]])}')
```

### Fibonacci encoding (SekaiCTF)
```python
fib=[1,2]
while fib[-1]<256:fib.append(fib[-1]+fib[-2])
encoded=[3,5,8,13,21,34,55,89]  # from binary
print(''.join(chr(fib.index(e)+0x20)for e in encoded))
```

## Real CTF References & Solve Examples

### Flare-On 2023 #3 (REOO — XOR decode)
```python
enc=[0x4e,0x6a,0x75,0x6d,0x6c,0x5b,0x67,0x70,0x7c,0x60,0x21,0x7e,0x77,0x76,0x7a,0x50,0x78,0x76]
key=0x37  # found via static analysis
print(f"Flag: {''.join(chr(b^key)for b in enc)}")
```

### SekaiCTF 2023 (Double Dabble — BCD decode)
```python
def reverse_bcd(v):
    r=[]
    while v>0:r.append(str(v&0xF));v>>=4
    return ''.join(reversed(r))
print(reverse_bcd(0x1234))  # from binary analysis
```

### SECCON 2023 (rev_easy — Python bytecode)
```python
import base64
enc=base64.b64decode("ChkcLyUoNyMlKmQlOiQrK24pCisrOiQpKzkjKzwjKz0lJz0jKysjKw==")
print(f"SECCON{{{''.join(chr(c^0x42)for c in enc)}}}")
```

## Binary Type Specific
```bash
# .NET
de4dot binary.exe -o clean.exe; ilspycmd clean.exe > dec.cs; grep -iE 'flag|secret' dec.cs
# Java
jadx binary.jar -d out/; grep -riE 'flag|secret' out/
# Python
uncompyle6 binary.pyc > dec.py; grep -i 'flag' dec.py
# Go
strings $FILE | grep -iE 'flag|secret'; go tool objdump binary | grep -A10 'main.main'
# Android
unzip app.apk lib/*/lib*.so; jadx app.apk -d out/
```

## Complete Pipeline
```bash
# 1.Classify 2.Unpack 3.Select 4.Solve
FILE=$1; file $FILE
strings $FILE | grep -qi UPX && upx -d $FILE
python3 -c "
import angr;proj=angr.Project('$FILE',auto_load_libs=False)
s=proj.factory.entry_state();sm=proj.factory.simgr(s)
sm.explore(find=lambda s:b'flag{'in s.posix.dumps(1),avoid=lambda s:b'wrong'in s.posix.dumps(1))
if sm.found:print(sm.found[0].posix.dumps(1))"
```

## Speed Metrics
```
String compare:<2min | XOR brute:<3min | Base64/Hex:<2min
Math transform:<5min | .NET/Java:<5min | Anti-debug:<10min
Packer detect:<3min | UPX unpack:<1min | VM analysis:<15min
Deobfuscation:<10min | Symbolic exec:<10min | Ghidra auto:<5min
```
