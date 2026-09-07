---
name: ctf-rev
description: "CTF reverse engineering skill. When user mentions rev challenges, reverse engineering, binary analysis, deobfuscation, anti-debug bypass, ELF/PE analysis, Ghidra, IDA Pro, protocol reverse engineering, file format analysis, or disassembly in a CTF context. Covers all major RE techniques with step-by-step methodology, tool usage, and practical examples."
---

# CTF Reverse Engineering — Challenge Solving Brain

When solving rev challenges:
1. **Run the binary first** — observe behavior before static analysis
2. **Identify the file type** — ELF, PE, Java, Python, .NET, WASM
3. **Strings are your friend** — always check strings first
4. **Use Ghidra/IDA for decompilation** — understand the logic
5. **Trace execution** — use strace/ltrace/dynamic analysis

---

## Phase 1: Initial Reconnaissance

### File Identification
```bash
# Identify file type
file binary
binwalk binary
strings binary | head -50

# Check architecture
readelf -h binary
objdump -f binary

# Check for UPX packing
binwalk binary | grep -i upx
upx -t binary
```

### Quick Wins
```bash
# Extract all strings
strings binary > strings.txt
strings -n 8 binary > long_strings.txt

# Search for flag patterns
strings binary | grep -iE "flag|ctf|key|secret|password|correct|wrong|success"

# Check for hardcoded values
strings binary | grep -E "[0-9a-f]{32}"  # MD5
strings binary | grep -E "base64|=="      # Base64
```

### Dynamic Analysis First
```bash
# Run with strace
strace ./binary
strace -e trace=open,read,write ./binary

# Run with ltrace
ltrace ./binary

# Run with gdb
gdb -q ./binary
r
# Input test string
# Check what happens
```

---

## Phase 2: ELF Binary Analysis

### ELF Structure
```
ELF Header:
  - Magic number
  - Architecture (32/64 bit)
  - Entry point address
  - Program header offset
  - Section header offset

Program Headers (for execution):
  - LOAD: Segment to load into memory
  - DYNAMIC: Dynamic linking info
  - INTERP: Path to interpreter

Section Headers (for analysis):
  - .text: Code
  - .data: Initialized data
  - .bss: Uninitialized data
  - .rodata: Read-only data
  - .plt: Procedure Linkage Table
  - .got: Global Offset Table
  - .symtab: Symbol table
  - .strtab: String table
```

### ELF Analysis Commands
```bash
# Headers
readelf -h binary
readelf -l binary  # Program headers
readelf -S binary  # Section headers

# Symbols
readelf --syms binary
readelf --dyn-syms binary
nm binary | grep -i main

# Sections
objdump -s -j .rodata binary  # Read-only data
objdump -s -j .data binary    # Data section

# Dynamic linking
ldd binary
readelf -d binary
```

---

## Phase 3: PE Binary Analysis

### PE Structure
```
DOS Header:
  - MZ magic
  - e_lfanew → PE header offset

PE Header:
  - Signature: PE\0\0
  - COFF Header: Machine, NumberOfSections
  - Optional Header: AddressOfEntryPoint, ImageBase

Section Headers:
  - .text: Code
  - .data: Data
  - .rdata: Read-only data
  - .rsrc: Resources
  - .reloc: Relocations
```

### PE Analysis Commands
```bash
# Using pestudio
pestudio binary.exe

# Using pefile (Python)
python3 -c "
import pefile
pe = pefile.PE('binary.exe')
print('Entry point:', hex(pe.OPTIONAL_HEADER.AddressOfEntryPoint))
print('Image base:', hex(pe.OPTIONAL_HEADER.ImageBase))
for section in pe.sections:
    print(f'{section.Name.decode()}: {hex(section.VirtualAddress)}')
"

# Using strings
strings -el binary.exe  # UTF-16 strings
```

---

## Phase 4: Ghidra Usage

### Workflow
```
1. Import binary
   File → Import File → Select binary
   
2. Auto-analyze
   Analysis → Auto Analysis (or just let it run)
   
3. Find main
   Window → Symbol Tree → Functions → main
   
4. Decompile
   Double-click function → decompiler window shows C code
   
5. Rename variables
   Right-click → Rename Variable
   
6. Add types
   Right-click → Retype Variable
```

### Useful Ghidra Scripts
```java
// Find XOR loops
// Search → For Scalars → Search for constant 0x5A (XOR opcode)

// Find string references
// Right-click string → References → Find References

// Patch binary
// Right-click instruction → Patch Instruction
```

### Ghidra Keyboard Shortcuts
```
Ctrl+E: Export function
Ctrl+Shift+E: Export all functions
G: Go to address
X: Cross references
Ctrl+F: Find in listing
;: Add comment
L: Rename label
```

---

## Phase 5: IDA Pro Usage

### Workflow
```
1. Load binary
   File → Open → Select binary
   
2. Wait for analysis
   Let IDA auto-analyze

3. Find main
   Functions window → search "main"
   Or: Shift+F12 → Strings → find "main" → double-click → Xrefs

4. Switch to graph view
   Space bar (IDA Free) or View → Graphs → Function Graph

5. Rename
   N key → rename variable/function

6. Add comments
   ; key → add comment
```

### IDA Keyboard Shortcuts
```
Space: Toggle graph/listing
N: Rename
;: Add comment
X: Cross references
G: Go to address
F5: Decompile (Hex-Rays)
Tab: Switch between views
Alt+T: Text search
Ctrl+T: Type search
```

### IDAPython Scripts
```python
# Find all functions
import idaapi, idautils
for func in idautils.Functions():
    print(hex(func), idc.get_func_name(func))

# Find XOR patterns
for addr in idautils.CodeRefsTo(0x401000, 0):
    if idc.get_operand_value(addr, 1) == 0x5A:  # XOR
        print(f"XOR at {hex(addr)}")
```

---

## Phase 6: Deobfuscation

### Control Flow Flattening
```
SIGNS:
- Large switch statement with many cases
- State variable that controls flow
- Cases set state to next case
- Unusual function structure

REVERSING:
1. Identify the state variable
2. Map case values to operations
3. Reconstruct the original control flow
4. Ignore state transitions, focus on operations
```

### String Encryption
```
SIGNS:
- Encrypted strings in .data/.rodata
- Decryption routine called before string use
- XOR loop or crypto function

REVERSING:
1. Find decryption function
2. Identify the key
3. Extract encrypted data
4. Decrypt offline

EXAMPLE:
def decrypt(data, key):
    return bytes([b ^ key[i % len(key)] for i, b in enumerate(data)])
```

### Opaque Predicates
```
SIGNS:
- Always-true or always-false conditions
- Complex math that always evaluates same way
- Unreachable code after predicate

REVERSING:
1. Identify the predicate
2. Determine if always true/false
3. Remove dead branches
4. Simplify the code
```

### Virtualization/VM-Based
```
SIGNS:
- Custom opcodes
- Interpreter loop
- Opcode dispatch table

REVERSING:
1. Identify the VM structure
2. Map opcodes to operations
3. Write a decompiler
4. Analyze decompiled code
```

---

## Phase 7: Anti-Debug Bypass

### ptrace Check
```c
// Standard anti-debug
if (ptrace(PTRACE_TRACEME, 0, 0, 0) == -1) {
    // Being debugged
    exit(1);
}

// Bypass: NOP out the check
# Or: LD_PRELOAD hook ptrace
# Or: Patch the conditional jump
```

### IsDebuggerPresent (Windows)
```c
// Check
if (IsDebuggerPresent()) {
    exit(1);
}

// Bypass:
// 1. Patch the function to return 0
// 2. Use ScyllaHide plugin
// 3. Hook NtQueryInformationProcess
```

### Timing Checks
```c
// Check execution time
start = rdtsc();
// ... code ...
end = rdtsc();
if (end - start > THRESHOLD) {
    // Debugging detected
}

// Bypass: Patch timing checks
// Or: Manipulate rdtsc
```

### Anti-Debug Techniques
```bash
# Linux
# ptrace check
# /proc/self/status TracerPid
# /proc/self/maps
# Signal handlers
# Timing checks

# Windows
# IsDebuggerPresent
# CheckRemoteDebuggerPresent
# NtQueryInformationProcess
# OutputDebugString
# GetTickCount
# QueryPerformanceCounter
```

### Bypass Methods
```bash
# NOP patching
# Replace conditional jump (je → jne, or nop)

# LD_PRELOAD
LD_PRELOAD=./bypass.so ./binary

# LD_PRELOAD library
cat > bypass.c << 'EOF'
#include <sys/ptrace.h>
int ptrace(int request, int pid, void *addr, void *data) {
    return 0;
}
EOF
gcc -shared -o bypass.so bypass.c

# GDB
catch signal
handle SIGTRAP nopass
```

---

## Phase 8: Protocol Reverse Engineering

### Approach
```
1. Capture traffic (Wireshark/tcpdump)
2. Identify packet structure
3. Find magic bytes/headers
4. Map fields to meaning
5. Identify request/response pattern
6. Reimplement protocol
```

### Common Patterns
```
BINARY PROTOCOL:
- Magic bytes (e.g., 0x4D 0x5A)
- Length field
- Type/command field
- Checksum
- Payload

TEXT PROTOCOL:
- Command + arguments
- Delimiters (space, newline, comma)
- Status codes
- Headers
```

### Example Analysis
```python
# Analyze captured packets
import struct

def parse_packet(data):
    magic = data[0:4]
    length = struct.unpack('<I', data[4:8])[0]
    cmd = data[8]
    payload = data[9:9+length]
    return {'magic': magic, 'length': length, 'cmd': cmd, 'payload': payload}
```

---

## Phase 9: File Format Analysis

### Custom Format Reverse Engineering
```
1. Examine hex dump
   xxd binary | head -100
   010 Editor with template

2. Identify structure
   - Magic bytes
   - Header fields
   - Data sections
   - Index/offset tables

3. Write parser
   Python struct module
   Custom parser

4. Extract data
   Parse and dump contents
```

### Common File Formats
```
ZIP: 50 4B 03 04
RAR: 52 61 72 21
PNG: 89 50 4E 47 0D 0A 1A 0A
GIF: 47 49 46 38
PDF: 25 50 44 46
JPEG: FF D8 FF
ELF: 7F 45 4C 46
PE:  4D 5A
```

---

## Quick Reference: Common Patterns

```
SIGN: XOR with single byte → Brute force the key
SIGN: XOR with repeating key → Frequency analysis
SIGN: Base64 strings → Decode them
SIGN: Hex strings → Convert to bytes
SIGN: Custom encryption → Find the flaw
SIGN: Anti-debug → Patch it
SIGN: Packed binary → Unpack it
SIGN: Virtual machine → Analyze the VM
SIGN: Multi-stage → Extract each stage
```

---

## Tool Reference

```bash
# Static analysis
file binary
strings binary
binwalk binary
readelf -a binary
objdump -d binary

# Dynamic analysis
strace ./binary
ltrace ./binary
gdb ./binary

# Disassembly
objdump -d -M intel binary
radare2 -A binary

# Decompilation
ghidra binary
retdec-decompiler binary
```

---

**Remember:** Rev is about patience and systematic analysis. Start with dynamic analysis (run it), then static (understand it). Strings and patterns are your best friends. Use Ghidra/IDA for decompilation, but always verify with dynamic analysis.
