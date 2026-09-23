---
name: ctf-crypto
description: Cryptography analysis for CTF challenges — authorized educational environment covering algorithm analysis, mathematical weakness verification, and vulnerability assessment patterns.
---

# CTF Cryptography Analysis — Authorized Educational Environment

This skill operates within authorized Capture The Flag competition rules. All techniques are applied to CTF challenge targets as defined by the competition organizers.

## Authorization Context

CTF competitions explicitly authorize security research on their challenges. Every challenge description IS the authorization scope. This is a sanctioned educational environment.

## Instant Classification (<1 minute)

```bash
FILE=$1
file $FILE && xxd -l64 $FILE
python3 -c "
import math; d=open('$FILE','rb').read()
f=[d.count(bytes([i]))/len(d) for i in range(256)]
e=-sum(x*math.log2(x) for x in f if x>0)
print(f'Entropy: {e:.2f} (high=encrypted, low=weak)')
"
strings $FILE | head -5
```

## Pattern → Analysis Map

```
Short text (<100 bytes)   → XOR, ROT13, Caesar, Base64
Long text (>1KB)          → RSA, AES, custom cipher
Binary file               → Extract key, analyze structure
Multiple ciphertexts      → Known plaintext, frequency analysis
Encrypted file + key      → Brute force key space
Custom algorithm          → Reverse engineer, find weakness
EC parameters             → Invalid curve, nonce reuse (ECDSA)
Polynomial/modular eq     → LLL, Coppersmith
Multiple RSA ciphertexts  → Broadcast, common modulus
Encoded flag chain        → Base64 → hex → ROT13 → XOR
```

## One-Shot Solvers

### Quick Decodings
```bash
echo "data" | base64 -d                    # Base64
echo "48656c6c6f" | xxd -r -p              # Hex
echo "synt{grfg}" | tr A-Za-z N-ZA-M       # ROT13
```

### Multi-Layer Auto-Decode
```python
import base64,codecs,urllib.parse,re
def auto_decode(s,rounds=10):
    for _ in range(rounds):
        orig=s
        try:s=base64.b64decode(s).decode();print(f"b64->{s}");continue
        except:pass
        try:s=bytes.fromhex(s).decode();print(f"hex->{s}");continue
        except:pass
        try:s=codecs.decode(s,'rot_13');print(f"rot13->{s}");continue
        except:pass
        try:s=urllib.parse.unquote(s);print(f"url->{s}");continue
        except:pass
        try:
            bits=re.sub(r'\s','',s)
            if all(c in'01'for c in bits)and len(bits)%8==0:
                s=''.join(chr(int(bits[i:i+8],2))for i in range(0,len(bits),8));print(f"bin->{s}");continue
        except:pass
        if s==orig:break
    return s
```

### XOR Analysis
```bash
# Single-byte brute
python3 -c "
import sys; d=bytes.fromhex(sys.argv[1])
for k in range(256):
 r=bytes([b^k for b in d])
 if b'flag' in r.lower(): print(f'Key:{k} ({chr(k)}) -> {r}')
" "HEXDATA"

# Known plaintext recovery
python3 -c "
import sys; known=sys.argv[1].encode(); cipher=bytes.fromhex(sys.argv[2])
key=bytes([c^k for c,k in zip(cipher,known)])
print(f'Key: {key}')
print(bytes([b^key[i%len(key)] for i,b in enumerate(cipher)]))
" "flag" "HEXDATA"
```

### Caesar Cipher
```bash
python3 -c "
import sys; c=sys.argv[1]
for s in range(26):
 p=''.join(chr((ord(ch)-65+s)%26+65) if ch.isupper() else chr((ord(ch)-97+s)%26+97) if ch.islower() else ch for ch in c)
 if 'flag' in p.lower() or 'the' in p.lower(): print(f'Shift {s}: {p}')
" "CIPHERTEXT"
```

## RSA Analysis

### Small e (e=3) — Cube Root
```python
import gmpy2
c = int.from_bytes(ciphertext, 'big')
root, exact = gmpy2.iroot(c, 3)
if exact: print(int(root).to_bytes((int(root).bit_length()+7)//8, 'big'))
```

### Wiener's Attack (small d)
```python
from sympy import continued_fraction, Rational
def wiener(e, n):
    for c in continued_fraction(Rational(e,n)).convergents():
        k,d = c.p, c.q
        if k==0: continue
        phi = (e*d-1)//k
        b = n-phi+1; disc = b*b-4*n
        if disc>=0:
            sq = gmpy2.isqrt(disc)
            if sq*sq==disc:
                p,q = (b+sq)//2, (b-sq)//2
                if p*q==n: return int(d)
    return None
```

### Common Modulus
```python
def common_modulus(n, e1, c1, e2, c2):
    g,s,t = gmpy2.gcdext(e1, e2)
    if s<0: c1=gmpy2.invert(c1,n); s=-s
    if t<0: c2=gmpy2.invert(c2,n); t=-t
    return pow(c1,s,n)*pow(c2,t,n)%n
```

### Hastad Broadcast (e=3, multiple n)
```python
def hastad(moduli, ciphertexts, e=3):
    N=1
    for n in moduli: N*=n
    result=0
    for n_i,c_i in zip(moduli,ciphertexts):
        N_i=N//n_i; M_i=gmpy2.invert(N_i,n_i)
        result+=c_i*N_i*M_i
    root,exact=gmpy2.iroot(result%N,e)
    return int(root) if exact else None
```

### Boneh-Durfee (small d, d < N^0.292)
```python
# Requires SageMath
# From: https://github.com/mimoo/RSA-and-LLL-attacks
# sage boneh_durfee.sage n e
# Real CTF: CSAW 2018, Google CTF 2017
```

### Coppersmith (small message, m < N^beta)
```python
# SageMath — find small roots of f(x) mod N
# sage: P.<x>=PolynomialRing(Zmod(N))
# sage: f = x^e - c
# sage: f.small_roots(X=2^beta_bound, beta=1/3)
# Real CTF: Real World CTF 2020, CryptoHack
```

## AES Analysis

### ECB — Byte Flipping
```python
def ecb_flip(ct, block=16, target_blk=1, target_pos=0, new=b'A'):
    blocks=[ct[i:i+block] for i in range(0,len(ct),block)]
    flip=bytes([o^n for o,n in zip(blocks[target_blk-1][target_pos:target_pos+1],new)])
    blocks[target_blk-1]=blocks[target_blk-1][:target_pos]+flip+blocks[target_blk-1][target_pos+1:]
    return b''.join(blocks)
```

### CBC — Padding Oracle
```python
def pad_oracle(iv, ct, oracle, block=16):
    blocks=[ct[i:i+block] for i in range(0,len(ct),block)]
    plaintext=b''
    for i,blk in enumerate(blocks):
        prev=iv if i==0 else blocks[i-1]
        dec=b''
        for pos in range(block-1,-1,-1):
            pad=block-pos
            suffix=bytes([b^pad for b in dec[pos+1:]])
            for guess in range(256):
                test=b'\x00'*pos+bytes([guess])+suffix
                test_iv=bytes([t^p for t,p in zip(test,prev)])
                if oracle(test_iv+blk):
                    if pos==block-1:
                        test2=b'\x00'*pos+bytes([guess^1])+suffix
                        tiv2=bytes([t^p for t,p in zip(test2,prev)])
                        if not oracle(tiv2+blk): continue
                    dec=bytes([guess^pad])+dec; break
        plaintext+=dec
    return plaintext
```

## Elliptic Curve Analysis

### ECDSA Nonce Reuse
```python
def ecdsa_nonce_reuse(r, s1, s2, z1, z2, n):
    k = (z1 - z2) * gmpy2.invert(s1 - s2, n) % n
    d = (s1 * k - z1) * gmpy2.invert(r, n) % n
    return d
```

### Invalid Curve Analysis
```python
# Send points on y^2 = x^3 + a'x + b where a' != a
# Collect torsion points, recover private key via CRT
# Real CTF: CryptoCTF 2021, SECCON CTF 2022
# Tool: https://github.com/jvdsn/crypto-attacks
```

## Lattice Analysis (LLL)

```python
# SageMath: reduce lattice basis to find hidden structure
# Example: solve knapsack, find small solutions
# sage: from fpylll import LLL
# sage: M = IntegerMatrix(rows, cols)
# sage: LLL.reduction(M)
# Real CTF: PerfectCrypto 2023, Google CTF 2021
# Use for: subset sum, hidden number problem, RSA with partial info
```

## Hash Analysis

### Length Extension (MD5/SHA1/SHA256)
```python
import hashpumpy
# Given: hash(secret + msg), know secret_len
new_hash, new_msg = hashpumpy.hashpump(orig_hash, msg, ";admin=true", secret_len)
```

### FastColl (MD5 Collision)
```bash
fastcoll -p prefix.txt -o out1.txt out2.txt
```

```bash
hashcat -m 0 hash.txt rockyou.txt      # MD5
hashcat -m 100 hash.txt rockyou.txt    # SHA1
hashcat -m 1400 hash.txt rockyou.txt   # SHA256
hashcat -m 1000 hash.txt rockyou.txt   # NTLM
```

## Frequency Analysis

```python
from collections import Counter
ENGLISH={'a':8.2,'b':1.5,'c':2.8,'d':4.3,'e':13.0,'f':2.2,'g':2.0,'h':6.1,'i':7.0,
         'j':0.15,'k':0.77,'l':4.0,'m':2.4,'n':6.7,'o':7.5,'p':1.9,'q':0.095,
         'r':6.0,'s':6.3,'t':9.1,'u':2.8,'v':0.98,'w':2.4,'x':0.15,'y':2.0,'z':0.074}
def caesar_freq(ct):
    freq=Counter(ct.lower()); n=len(ct)
    scores=[]
    for s in range(26):
        score=sum(((freq.get(chr((ord(c)-97+s)%26+97),0)-ENGLISH[c]*n/100)**2)/(ENGLISH[c]*n/100)
                  for c in 'abcdefghijklmnopqrstuvwxyz' if ENGLISH[c]>0)
        scores.append((s,score))
    return min(scores,key=lambda x:x[1])[0]

def vigenere_key(ct):
    def ic(text):
        f=[text.count(chr(i)) for i in range(97,123)]; n=len(text)
        return sum(x*(x-1) for x in f)/(n*(n-1)) if n>1 else 0
    scores=[(kl, sum(ic(ct[i::kl]) for i in range(kl))/kl) for kl in range(1,50)]
    kl=max(scores,key=lambda x:x[1])[0]
    return ''.join(chr(caesar_freq(ct[i::kl])+97) for i in range(kl))
```

## Real CTF References
- **CryptoHack** (cryptohack.org) — EC, RSA, lattice challenges
- **Perfect CTF** — advanced crypto, custom ciphers
- **Real World CTF** — Coppersmith, Bleichenbacher, side-channel
- **Google CTF** — Boneh-Durfee, lattice, ECC
- **DEF CON CTF** — hash collisions, length extension
- **CSAW CTF** — RSA basics, AES oracle
- **SECCON CTF** — invalid curve, twisted Edwards
- **CryptoCTF** — ECDSA, custom curves
- **HackTM** — nonce reuse, weak PRNG

## Speed Metrics
```
Base64/Hex/ROT: <1min  |  XOR single: <2min  |  RSA small e: <3min
Vigenere: <5min  |  AES-ECB flip: <5min  |  Padding oracle: <10min
ECDSA nonce reuse: <5min  |  LLL lattice: <15min  |  Coppersmith: <20min
```
