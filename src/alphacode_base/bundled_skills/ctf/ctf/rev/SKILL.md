# CTF Reverse Engineering Skill

## Speed-First Approach: Solve rev challenges in <15 minutes

### Phase 1: Instant Classification (<2 minutes)
```bash
# Run this immediately on any binary challenge
FILE=$1

# Basic info
file $FILE
strings -n8 $FILE | head -30
strings $FILE | grep -iE 'flag\{|ctf\{|CTF\{|FLAG\{|correct\|wrong\|success'

# Check for obvious patterns
strings $FILE | grep -E '[A-Za-z0-9]{20,}' | head -10
strings $FILE | grep -i 'password\|key\|secret\|admin'

# Entropy check (high = packed/encrypted, low = simple)
python3 -c "
import math
data=open('$FILE','rb').read()
freq=[data.count(bytes([i]))/len(data) for i in range(256)]
e=-sum(f*math.log2(f) for f in freq if f>0)
print(f'Entropy: {e:.2f} bits/byte')
print('Likely: Packed/Encrypted' if e > 7.5 else 'Likely: Normal' if e < 6.5 else 'Likely: Mixed')
"

# File structure
xxd -l64 $FILE
readelf -h $FILE 2>/dev/null || file -b $FILE
```

### Phase 2: Tool Selection (<1 minute)
```
MATCH binary to tool:
├── ELF binary → Ghidra, IDA, radare2
├── .NET → dnSpy, dotPeek, ILSpy
├── Java → JD-GUI, JADX, CFR
├── Python bytecode (.pyc) → uncompyle6, decompyle3
├── .NET Assembly (.exe/.dll) → dnSpy
├── Go binary → go tool objdump, Ghidra
├── Rust binary → Ghidra,IDA
├── Android (dex/apk) → jadx-gui, apktool
├── JavaScript → node --inspect, de4js
└── Obfuscated → angr (symbolic execution)
```

### Phase 3: Solve and Submit
```
IF flag found → submit immediately
ELSE → try next tool (come back later if stuck)
```

## One-Liner Solvers

### Quick String Extraction
```bash
# Extract all printable strings
strings -n8 $FILE

# Extract strings with context
strings -t x $FILE | head -30  # Show offset

# Extract specific patterns
strings $FILE | grep -oP 'flag\{[^}]+\}'

# Extract base64 strings
strings $FILE | grep -Ei '[A-Za-z0-9+/]{20,}={0,2}' | while read s; do
  echo "$s" | base64 -d 2>/dev/null | grep -qi flag && echo "B64: $s"
done

# Extract hex strings
strings $FILE | grep -Ei '^[0-9a-f]{20,}$' | while read s; do
  echo "$s" | xxd -r -p 2>/dev/null | grep -qi flag && echo "HEX: $s"
done
```

### Quick Binary Analysis
```bash
# Function list
objdump -t $FILE | grep -i 'main\|flag\|win\|check\|verify\|decrypt'

# Import list
objdump -p $FILE | grep NEEDED
nm -D $FILE | grep -i 'printf\|puts\|gets\|scanf\|strcmp'

# Disassemble main
objdump -d $FILE | grep -A50 '<main>:'

# Check for obfuscation
objdump -d $FILE | grep -c 'xor\|rol\|ror' | xargs echo "XOR/ROT count:"
```

### Ghidra Headless Analysis
```bash
# Run Ghidra headless (fast)
analyzeHeadless /tmp/ghidra_project project_name \
  -import $FILE \
  -postScript FindFlag.java \
  -scriptPath /path/to/scripts

# Find flag strings
find /tmp/ghidra_project -name "*.rep" -exec grep -l "flag{" {} \;
```

## Fast Analysis Patterns

### Pattern: Simple XOR Encoding
```
Detection: strings with uniform distribution, not English
Attack:
1. Find XOR loop in disassembly
2. Identify key (usually single byte or short)
3. Brute force key
```

```python
# Quick XOR decode
data = open('binary', 'rb').read()
for key in range(256):
    result = bytes([b ^ key for b in data])
    if b'flag' in result.lower():
        print(f'Key: {key} ({chr(key)}) → {result}')
```

### Pattern: Base64 in Binary
```
Detection: Long base64 strings in strings output
Attack:
1. Find base64 string
2. Decode
3. Often combined with XOR or other transforms
```

```bash
# Extract and decode base64
strings $FILE | grep -Ei '[A-Za-z0-9+/]{40,}={0,2}' | while read s; do
  decoded=$(echo "$s" | base64 -d 2>/dev/null)
  [ -n "$decoded" ] && echo "$s → $decoded"
done
```

### Pattern: Simple Comparison
```
Detection: strcmp(), strncmp() with hardcoded string
Attack:
1. Find strcmp call in disassembly
2. Extract second argument (hardcoded string)
3. That's the password/flag
```

```bash
# Find strcmp comparisons
objdump -d $FILE | grep -B5 -A5 'strcmp'

# Or in Ghidra: search for xrefs to strcmp
# Input string is first arg, hardcoded is second
```

### Pattern: Math Operations
```
Detection: XOR, ADD, SUB, ROL, ROR on each character
Attack:
1. Identify operation
2. Find key/constant
3. Apply inverse operation
```

```python
# Brute force simple math transforms
import itertools

def try_transforms(data):
    # Try common transforms
    for key in range(256):
        # XOR
        if bytes([b ^ key for b in data[:20]]).isascii():
            print(f'XOR {key}: {bytes([b ^ key for b in data])}')
        
        # ADD/SUB
        for op in [operator.add, operator.sub]:
            result = bytes([op(b, key) & 0xFF for b in data[:20]])
            if result.isascii():
                print(f'{"ADD" if op == operator.add else "SUB"} {key}: {result}')
```

### Pattern: Control Flow Flattening
```
Detection: Large switch statement in disassembly
Attack:
1. Identify state variables
2. Trace execution path
3. Reconstruct original logic
```

### Pattern: OLLVM (Obfuscated LLVM)
```
Detection: Opaque predicates, bogus control flow
Attack:
1. Use symbolic execution (angr)
2. Simplify with Ghidra/IDA decompiler
3. Dynamic analysis (GDB + script)
```

## Decompilation Tools

### Ghidra Usage
```bash
# Install Ghidra
wget https://github.com/NationalSecurityAgency/ghidra/releases/latest
unzip ghidra_*.zip
./ghidra_*/ghidraRun

# Headless analysis
analyzeHeadless /tmp/project proj -import binary -postScript FlagFinder.java

# Ghidra script to find flags
# FlagFinder.java
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.data.*;
import ghidra.program.model.mem.*;

public class FlagFinder extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Search for flag pattern
        Memory mem = currentProgram.getMemory();
        byte[] buf = new byte[1000];
        for (AddressSetView set : mem.getAddresses(true)) {
            // Search for "flag{" bytes
            // ...
        }
    }
}
```

### IDA Free Usage
```bash
# Install IDA Free
# Download from hex-rays.com/ida-free

# Analyze binary
ida64 binary

# Key shortcuts:
# Space: Toggle graph/text
# F5: Decompile (if available)
# X: References to
# N: Rename
# G: Jump to address
```

### radare2 Usage
```bash
# Quick analysis
r2 -A $FILE

# Commands
aaa              # Analyze all
afl              # List functions
pdf @main        # Disassemble main
axt @sym.flag    # Find xrefs to flag function
iz               # List strings
izz              # List all strings
px 64 @rsp       # Print hex at stack
wao xor          # Apply XOR obfuscation

# Find flag
/ flag{
/ flag
izz~flag
```

### angr Usage
```python
import angr

# Load binary
proj = angr.Project('./binary', auto_load_libs=False)

# Create initial state
state = proj.factory.entry_state()

# Create simulation manager
simgr = proj.factory.simgr(state)

# Find path to "Correct!" or "flag{" 
simgr.explore(find=0x401234, avoid=0x401256)  # Use actual addresses

if simgr.found:
    found = simgr.found[0]
    print(found.posix.dumps(0))  # stdin
    print(found.memory.dumps(0x400000, 0x100))  # Memory
```

### JEB Usage (Android/Dalvik)
```bash
# Install JEB
# Download from pnfsoftware.com

# Analyze APK
jeb_binary binary.apk

# Navigate:
# - Code browser: Decompiled Java
# - Analysis: Strings, xrefs
# - Search: Search for "flag"
```

## Binary Type Specific Approaches

### .NET Binary
```bash
# Decompile with dnSpy
# Open in dnSpy → File → Open → select binary

# Or use ILSpy
ilspycmd binary.exe > decompiled.cs

# Search for flag
grep -i 'flag\|password\|secret' decompiled.cs

# Key patterns:
# - Compare: if (input == "flag{...}")
# - XOR: input[i] ^ key
# - Base64: Convert.FromBase64String()
```

### Java Binary
```bash
# Decompile with JD-GUI
jd-gui binary.jar

# Or use jadx
jadx binary.jar -d output/

# Search for flag
grep -ri 'flag\|password' output/

# Key patterns:
# - String.equals()
# - StringBuilder.append()
# - Integer.parseInt()
```

### Python Bytecode
```bash
# Decompile .pyc
uncompyle6 binary.pyc > decompiled.py

# Or use decompyle3
decompyle3 binary.pyc > decompiled.py

# Search for flag
grep -i 'flag\|password' decompiled.py

# Key patterns:
# - XOR: ord(c) ^ key
# - Base64: base64.b64decode()
# - Hash: hashlib.md5().hexdigest()
```

### Go Binary
```bash
# Go binaries have lots of strings
strings $FILE | grep -i 'flag\|password\|secret'

# Use Ghidra with Go analyzer
# Or use go tool objdump
go tool objdump binary | grep -A10 'main.main'

# Key patterns:
# - fmt.Println("flag{...}")
# - strings.Contains(input, "flag")
# - XOR with key
```

### JavaScript (Node.js)
```bash
# If provided .js file
node --inspect binary.js

# Or use de4js
# Online: https://www.cleancss.com/js-deobfuscator/

# Key patterns:
# - eval() with encoded string
# - atob() for base64
# - String.fromCharCode() for char conversion
```

## Symbolic Execution with angr

### Basic angr Template
```python
import angr
import claripy

def solve_with_angr(binary_path, find_addr, avoid_addr=None):
    proj = angr.Project(binary_path, auto_load_libs=False)
    
    # Create symbolic input
    state = proj.factory.entry_state()
    simgr = proj.factory.simgr(state)
    
    # Explore
    if avoid_addr:
        simgr.explore(find=find_addr, avoid=avoid_addr)
    else:
        simgr.explore(find=find_addr)
    
    if simgr.found:
        found = simgr.found[0]
        # Extract stdin
        return found.posix.dumps(0)
    return None

# Usage
result = solve_with_angr('./binary', 0x401234, 0x401256)
if result:
    print(f"Input: {result}")
```

### angr with Constraints
```python
import angr
import claripy

proj = angr.Project('./binary', auto_load_libs=False)

# Create symbolic bitvector
input_size = 30
input_bytes = claripy.BVS('input', input_size * 8)

state = proj.factory.full_init_state(
    args=['./binary'],
    stdin=angr.SimFileStream(name='stdin', content=input_bytes)
)

# Add constraints
for i in range(input_size):
    # Only printable ASCII
    state.solver.add(input_bytes.get_byte(i) >= 0x20)
    state.solver.add(input_bytes.get_byte(i) <= 0x7e)

simgr = proj.factory.simgr(state)
simgr.explore(find=0x401234, avoid=0x401256)

if simgr.found:
    found = simgr.found[0]
    solution = found.solver.eval(input_bytes, cast_to=bytes)
    print(f"Flag: {solution}")
```

## Z3 Constraint Solving

### Basic Z3 Template
```python
from z3 import *

# Create solver
s = Solver()

# Create variables
flag = [BitVec(f'f{i}', 8) for i in range(30)]

# Add constraints (from reverse engineering)
# Example: each character XORed with key
key = 0x42
for i in range(30):
    s.add(flag[i] ^ key == expected[i])  # expected from binary

# Add ASCII constraints
for f in flag:
    s.add(f >= 0x20, f <= 0x7e)

# Solve
if s.check() == sat:
    m = s.model()
    result = ''.join(chr(m[f].as_long()) for f in flag)
    print(f"Flag: {result}")
```

### Z3 for Complex Logic
```python
from z3 import *

s = Solver()

# Variables
x, y, z = BitVecs('x y z', 32)

# Constraints from binary analysis
s.add(x ^ y == 0x12345678)
s.add((x + y) * z == 0x9abcdef0)
s.add(x > 0, y > 0, z > 0)

if s.check() == sat:
    m = s.model()
    print(f"x = {m[x]}")
    print(f"y = {m[y]}")
    print(f"z = {m[z]}")
```

## Speed Metrics
```
Average solve times (target):
- Simple string comparison: <2 minutes
- XOR with known key: <3 minutes
- Base64 in binary: <2 minutes
- Simple math transform: <5 minutes
- .NET/Java decompilation: <5 minutes
- Complex obfuscation: <15 minutes
- OLLVM: <20 minutes
```
