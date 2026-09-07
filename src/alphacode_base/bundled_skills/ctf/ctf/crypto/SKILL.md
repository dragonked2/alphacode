---
name: ctf-crypto
description: "CTF cryptanalysis skill. When user mentions crypto CTF challenges, cryptanalysis, RSA attacks, AES/CBC attacks, hash length extension, padding oracle, elliptic curve attacks, PRNG prediction, one-time pad reuse, or mathematical crypto challenges. Covers all major cryptographic attack vectors with step-by-step methodology, formulas, and code examples."
---

# CTF Cryptanalysis — Challenge Solving Brain

When solving crypto CTF challenges:
1. **Identify the algorithm** — what crypto is being used?
2. **Check for implementation flaws** — most CTF crypto breaks are implementation errors
3. **Analyze the math** — reduce to a solvable equation
4. **Use SageMath/Python** — most crypto solutions need code
5. **Check common attacks first** — don't overcomplicate

---

## Phase 1: Identification

### Algorithm Detection
```
CHECKLIST:
□ Read the provided source code (most crypto challenges give source)
□ Identify the algorithm (RSA, AES, ECC, custom)
□ Check key sizes and parameters
□ Look for non-standard implementations
□ Check for known vulnerabilities in the implementation
□ Identify the mode of operation (ECB, CBC, CTR, GCM)
```

### Common CTF Crypto Patterns
```
PATTERN: RSA with small public exponent (e=3) → Hastad's broadcast attack
PATTERN: RSA with large public exponent → Wiener's attack
PATTERN: RSA with common modulus → Common modulus attack
PATTERN: AES-ECB → ECB oracle or byte-at-a-time
PATTERN: AES-CBC → Padding oracle or bitflipping
PATTERN: Custom PRNG → Predict the state
PATTERN: One-time pad used twice → Crib dragging
PATTERN: Hash with secret key → Hash length extension
PATTERN: XOR with repeating key → Frequency analysis
PATTERN: Base64/hex/rot13 → Simple encoding (check first!)
```

---

## Phase 2: RSA Attacks

### RSA Basics
```
Key Generation:
p, q = random primes
n = p * q
φ(n) = (p-1)(q-1)
e * d ≡ 1 (mod φ(n))

Encryption: c = m^e mod n
Decryption: m = c^d mod n
```

### Wiener's Attack (Large e)
```python
from Crypto.PublicKey import RSA
from ContinuedFraction import rational_to_contfrac, convergents_of_contfrac

def wiener_attack(e, n):
    # When d is small (d < n^0.25)
    cf = rational_to_contfrac(e, n)
    convergents = convergents_of_contfrac(cf)
    
    for k, d in convergents:
        if k == 0:
            continue
        phi = (e * d - 1) // k
        # Check if phi is valid
        b = n - phi + 1
        discriminant = b * b - 4 * n
        if discriminant >= 0:
            sqrt_d = isqrt(discriminant)
            if sqrt_d * sqrt_d == discriminant:
                p = (b + sqrt_d) // 2
                q = (b - sqrt_d) // 2
                if p * q == n:
                    return d
    return None
```

### Hastad's Broadcast Attack (Small e, Same m)
```python
# When same message encrypted with same e but different n
# e=3, 3 different ciphertexts
from Crypto.PublicKey import RSA
from sympy import integer_nthroot

def hastad_attack(ciphertexts, moduli, e=3):
    # Chinese Remainder Theorem
    from sympy.ntheory.modular import crt
    N = crt(moduli, ciphertexts)[0]
    
    # e-th root
    m, exact = integer_nthroot(N, e)
    if exact:
        return m
    return None

# Usage
c1, c2, c3 = [...], [...], [...]
n1, n2, n3 = [...], [...], [...]
flag = hastad_attack([c1, c2, c3], [n1, n2, n3])
print(long_to_bytes(flag))
```

### Common Modulus Attack (Same n, Different e)
```python
# Same n, different e, same plaintext
# c1 = m^e1 mod n, c2 = m^e2 mod n
# If gcd(e1, e2) = 1: s*e1 + t*e2 = 1
# m = c1^s * c2^t mod n

from Crypto.PublicKey import RSA
from sympy import mod_inverse

def common_modulus(e1, e2, c1, c2, n):
    g, s, t = extended_gcd(e1, e2)
    if g != 1:
        return None
    m = (pow(c1, s, n) * pow(c2, t, n)) % n
    return m
```

### Boneh-Durfee Attack (Small d, d < n^0.292)
```python
# More powerful than Wiener's, requires SageMath
# See: https://github.com/mimoo/RSA-and-LLL-attacks
```

### Fermat's Factoring (Close Primes)
```python
import gmpy2

def fermat_factor(n):
    a = gmpy2.isqrt(n) + 1
    b2 = a * a - n
    while not gmpy2.is_square(b2):
        a += 1
        b2 = a * a - n
    b = gmpy2.isqrt(b2)
    return int(a - b), int(a + b)

p, q = fermat_factor(n)
```

### Known High Bits of p
```python
# Coppersmith's method (SageMath)
# If we know half the bits of p
def known_high_bits(n, p_high, num_known_bits):
    P.<x> = PolynomialRing(Zmod(n))
    f = p_high + x
    p = f.small_roots(X=2^num_known_bits, beta=0.4)
    return p
```

---

## Phase 3: AES/CBC/ECB Attacks

### ECB Byte-at-a-Time
```python
# When user controls plaintext and can observe ECB output
# Block size = 16 bytes

def ecb_oracle(plaintext):
    # Encrypts with ECB, returns ciphertext
    pass

def find_block_size():
    for i in range(1, 64):
        c1 = ecb_oracle(b'A' * i)
        c2 = ecb_oracle(b'A' * (i + 1))
        if len(c1) != len(c2):
            return len(c2) - len(c1)

def ecb_decrypt():
    block_size = 16
    known = b''
    for pos in range(block_size):
        block = b'A' * (block_size - 1 - pos)
        target = ecb_oracle(b'A' * block_size)[:block_size]
        for byte in range(256):
            if ecb_oracle(block + known + bytes([byte]))[:block_size] == target:
                known += bytes([byte])
                break
    return known
```

### ECBOracle Detection
```python
def is_ecb(ciphertext, block_size=16):
    blocks = [ciphertext[i:i+block_size] for i in range(0, len(ciphertext), block_size)]
    return len(blocks) != len(set(blocks))
```

### CBC Bitflipping
```python
# Modify ciphertext to change plaintext in predictable way
# In CBC: P[i] = D(C[i]) XOR C[i-1]
# So flipping C[i-1] flips P[i]

def cbc_bitflip(ciphertext, target_change, block_num, iv=None):
    # If we want to change plaintext from "old" to "new"
    # at position block_num
    blocks = [iv] + [ciphertext[i:i+16] for i in range(0, len(ciphertext), 16)]
    
    # XOR old plaintext with new plaintext
    diff = xor(target_change['old'], target_change['new'])
    
    # Apply to previous ciphertext block
    modified_block = xor(blocks[block_num - 1], diff)
    blocks[block_num - 1] = modified_block
    
    return b''.join(blocks[1:])  # Exclude IV
```

### CBC Padding Oracle
```python
# When server reveals padding validity
def padding_oracle(ciphertext, iv, oracle):
    plaintext = b''
    for block_idx in range(len(ciphertext) // 16):
        block = ciphertext[block_idx*16:(block_idx+1)*16]
        decrypted_block = b''
        
        for byte_idx in range(15, -1, -1):
            padding_val = 16 - byte_idx
            prefix = xor(decrypted_block, bytes([padding_val] * len(decrypted_block)))
            
            for guess in range(256):
                modified_block = block[:byte_idx] + bytes([guess]) + xor(block[byte_idx+1:], bytes([padding_val] * (15-byte_idx)))
                if oracle(iv + modified_block):
                    decrypted_block = bytes([guess ^ block[byte_idx] ^ padding_val]) + decrypted_block
                    break
        
        plaintext += decrypted_block
        iv = block
    
    return plaintext[:-plaintext[-1]]  # Remove padding
```

### CBC IV Recovery
```python
# If IV is predictable, can manipulate first block
# P[0] = D(C[0]) XOR IV
# Flipping IV bits directly flips P[0] bits
```

---

## Phase 4: Hash Length Extension

### When Applicable
```
Hash = MD5(secret + user_data)
Attacker can extend to: Hash = MD5(secret + user_data + padding + extension)
without knowing the secret
```

### Attack
```python
from hashpumpy import hashpump

original_hash = "..."
original_data = "..."
extension = ";admin=true"

new_hash, new_data = hashpump(original_hash, original_data, extension, len(secret))
# new_hash is valid for new_data = original_data + padding + extension
```

### Manual Implementation
```python
import struct
import hashlib

def md5_extend(original_hash, original_length, append_data):
    # Initialize with original hash
    md5 = hashlib.md5()
    md5.state = struct.unpack('<4I', bytes.fromhex(original_hash))
    
    # Craft padding
    padding = b'\x80' + b'\x00' * ((55 - original_length) % 64) + struct.pack('<Q', original_length * 8)
    
    # Update with extension
    md5.update(padding + append_data)
    
    return md5.hexdigest()
```

---

## Phase 5: Elliptic Curve Attacks

### Invalid Curve Attack
```python
# When server doesn't validate points are on curve
# Can use points from different curves to recover private key

# Point on curve: y^2 = x^3 + ax + b
# If server doesn't check, can use: y^2 = x^3 + ax + b' (different b)
def invalid_curve_attack(server_oracle, curve):
    points = []
    for x in range(100):
        # Try multiple b values
        for b in range(100):
            try:
                y_sq = (x**3 + curve.a * x + b) % curve.p
                y = modular_sqrt(y_sq, curve.p)
                points.append((x, y))
            except:
                continue
    return points
```

### Small Subgroup Attack
```python
# If point has small order, can brute force
def small_subgroup_attack(E, G, Q):
    for i in range(1, 1000):
        try:
            # Small order points
            P = E.random_point()
            # Check order
        except:
            continue
```

### MOV Attack (Small Embedding Degree)
```python
# When embedding degree k is small
# Transfers ECDLP to finite field DLP
def mov_attack(E, G, Q, k):
    # k is embedding degree
    # Pairing: e(k*G, Q) = e(G, k*Q)
    # This is in F_{p^k}*
    pass
```

### Pohlig-Hellman (Smooth Order)
```python
# When group order factors into small primes
def pohlig_hellman(G, Q, order):
    factors = factor(order)
    results = []
    for p, e in factors:
        # Solve in subgroup of order p^e
        G_sub = (order // (p**e)) * G
        Q_sub = (order // (p**e)) * Q
        # Brute force small subgroup
        for i in range(p**e):
            if i * G_sub == Q_sub:
                results.append((i, p**e))
                break
    # CRT to combine
    return crt([r[0] for r in results], [r[1] for r in results])
```

---

## Phase 6: PRNG Prediction

### Mersenne Twister Prediction
```python
# If 624 consecutive 32-bit outputs are observed
import random
import hashlib

def untemper(y):
    y ^= y >> 18
    y ^= (y << 15) & 0xefc60000
    y ^= (y << 7) & 0x9d2c5680
    y ^= y >> 11
    return y

def clone_mt19937(outputs):
    state = [untemper(o) for o in outputs]
    # Create new Random with recovered state
    r = random.Random()
    r.setstate((3, tuple(state + [0] * (624 - len(state))), None))
    return r
```

### Linear Congruential Generator
```python
# X[n+1] = (a * X[n] + c) mod m
# Given 2 consecutive outputs, recover a and c
def crack_lcg(x0, x1, x2):
    # a = (x2 - x1) / (x1 - x0) mod m
    # c = x1 - a * x0 mod m
    # Need to solve modular linear equations
    pass
```

### time() Based PRNG
```python
import time
import random

# If seed is time-based
for offset in range(-10, 10):
    random.seed(int(time.time()) + offset)
    # Try to match output
```

---

## Phase 7: One-Time Pad Reuse

### Crib Dragging
```python
def crib_drag(c1, c2, crib):
    # c1 XOR c2 = p1 XOR p2
    # If we guess part of p1 (the crib), we get part of p2
    xor_result = bytes(a ^ b for a, b in zip(c1, c2))
    partial_plaintext = bytes(a ^ b for a, b in zip(xor_result, crib))
    return partial_plaintext

# Try common cribs
cribs = [b'the ', b'flag{', b'Flag{', b'FLAG{', b'and ', b' is ']
for crib in cribs:
    result = crib_drag(c1, c2, crib)
    if result.isprintable():
        print(f"Crib '{crib}': {result}")
```

### Two-Time Pad Key Recovery
```python
# If same key used for two messages
# k = m1 XOR c1
# k = m2 XOR c2
# If we know m1, we get k = m1 XOR c1
# Then m2 = k XOR c2
```

---

## Phase 8: XOR Challenges

### Single-byte XOR
```python
def single_byte_xor(data):
    best = None
    best_score = -1
    for key in range(256):
        decrypted = bytes(b ^ key for b in data)
        score = english_score(decrypted)
        if score > best_score:
            best_score = score
            best = decrypted
    return best

def english_score(data):
    freq = 'ETAOINSHRDLCUMWFGYPBVKJXQZ'
    score = 0
    for byte in data:
        char = chr(byte).upper()
        if char in freq:
            score += freq.index(char)
    return score
```

### Repeating-key XOR
```python
def repeating_key_xor(ciphertext, known_plaintext, position):
    return bytes(c ^ k for c, k in zip(ciphertext[position:position+len(known_plaintext)], known_plaintext))

# To find key length
def find_key_length(ciphertext, max_length=40):
    scores = []
    for kl in range(2, max_length + 1):
        chunks = [ciphertext[i:i+kl] for i in range(0, len(ciphertext), kl)]
        score = sum(hamming_distance(a, b) for a, b in zip(chunks, chunks[1:])) / (len(chunks) - 1)
        scores.append((score / kl, kl))
    return sorted(scores)[0][1]

def hamming_distance(a, b):
    return sum(bin(x ^ y).count('1') for x, y in zip(a, b))
```

---

## Phase 9: Hash Attacks

### MD5 Collision
```python
import hashlib

# Generate MD5 collision
# Use fastcoll tool
# fastcoll -o prefix1 prefix2
```

### Length Extension
```python
# Already covered in Phase 4
# Works on: MD5, SHA-1, SHA-256 (not SHA-3)
```

### Rainbow Tables
```bash
# Crack with rainbow tables
# Project RainbowCrack
rcracki_mt -h hash.txt tables/
```

---

## Quick Reference: Common Formulas

```
RSA:
  n = p * q
  φ(n) = (p-1)(q-1)
  e*d ≡ 1 (mod φ(n))
  c = m^e mod n
  m = c^d mod n

Fermat:
  a^p ≡ a (mod p) for prime p

Euler:
  a^φ(n) ≡ 1 (mod n) for gcd(a,n)=1

Chinese Remainder Theorem:
  x ≡ a1 (mod m1)
  x ≡ a2 (mod m2)
  x = Σ(ai * Mi * yi) mod M
  where M = m1*m2*...*mn
  Mi = M/mi
  yi = Mi^(-1) mod mi
```

---

## SageMath Snippets

```python
# Factor n
factor(n)

# Discrete log
discrete_log(G, Q, operation='additive')

# Chinese Remainder Theorem
crt([a1, a2, a3], [m1, m2, m3])

# Polynomial roots over finite field
P.<x> = PolynomialRing(Zmod(n))
f = x^2 + a*x + b
f.roots()

# Small roots (Coppersmith)
f.small_roots(X=2^128, beta=0.4)

# LLL reduction
M = matrix([...])
L = M.LLL()
```

---

**Remember:** CTF crypto is usually about implementation flaws, not breaking the math. Check for: small exponents, known plaintext, padding issues, key reuse, weak PRNG, and implementation bugs. Use SageMath for heavy math.
