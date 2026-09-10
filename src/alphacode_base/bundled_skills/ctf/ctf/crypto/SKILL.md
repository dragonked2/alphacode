# CTF Cryptography Skill

## Speed-First Approach: Solve crypto challenges in <10 minutes

### Phase 1: Instant Classification (<2 minutes)
```bash
# Run this immediately on any crypto challenge
FILE=$1

# File type detection
file $FILE
xxd -l64 $FILE

# Entropy check (high = encrypted/compressed, low = weak crypto)
python3 -c "
import math
data=open('$FILE','rb').read()
freq=[data.count(bytes([i]))/len(data) for i in range(256)]
e=-sum(f*math.log2(f) for f in freq if f>0)
print(f'Entropy: {e:.2f} bits/byte (max 8.0)')
print('Class: Weak' if e < 4.5 else 'Class: Strong' if e > 7.5 else 'Class: Moderate')
"

# Quick pattern recognition
strings $FILE | head -5
xxd $FILE | head -20
```

### Phase 2: Pattern Matching (<3 minutes)
```
MATCH challenge to known pattern:
├── Short text (<100 bytes) → XOR, ROT13, Caesar, Base64
├── Long text (>1KB) → RSA, AES, custom cipher
├── Binary file → Extract key, analyze structure
├── Multiple ciphertexts → Known plaintext, frequency analysis
├── Encrypted file + key → Brute force key space
└── Custom algorithm → Reverse engineer, find weakness
```

### Phase 3: Solve and Submit
```
IF flag found → submit immediately
ELSE → try next attack (come back later if stuck)
```

## One-Liner Solvers

### Quick Decodings
```bash
# Base64 decode
echo "eyJmbGFnIjoiVEVTVCJ9" | base64 -d

# Hex decode
echo "48656c6c6f" | xxd -r -p

# ROT13
echo "synt{grfg}" | tr A-Za-z N-ZA-M

# URL decode
python3 -c "import urllib.parse; print(urllib.parse.unquote('flag%7Btest%7D'))"

# Binary to text
python3 -c "print(int('0100100001101001',2).to_bytes(2,'big').decode())"
```

### XOR Attacks
```bash
# Single-byte XOR brute force
python3 -c "
import sys
data = bytes.fromhex(sys.argv[1])
for key in range(256):
    result = bytes([b ^ key for b in data])
    if b'flag' in result.lower() or b'ctf' in result.lower():
        print(f'Key: {key} ({chr(key)}) → {result.decode(errors=\"ignore\")}')
" "4a5b5c5d5e5f50"

# Known plaintext XOR recovery
python3 -c "
import sys
known = sys.argv[1].encode()
cipher = bytes.fromhex(sys.argv[2])
key = bytes([c ^ k for c, k in zip(cipher, known)])
print(f'Recovered key: {key}')
print(f'Full decrypt: {bytes([b ^ key[i % len(key)] for i, b in enumerate(cipher)])}')
" "flag" "0a1b2c3d"

# XOR with repeating key
python3 -c "
import sys
key = sys.argv[1].encode()
data = bytes.fromhex(sys.argv[2])
print(bytes([b ^ key[i % len(key)] for i, b in enumerate(data)]))
" "SECRET" "0a1b2c3d4e5f"
```

### Caesar Cipher
```bash
# Brute force all 25 shifts
python3 -c "
import sys
cipher = sys.argv[1]
for shift in range(26):
    plain = ''
    for c in cipher:
        if c.isalpha():
            base = ord('A') if c.isupper() else ord('a')
            plain += chr((ord(c) - base + shift) % 26 + base)
        else:
            plain += c
    if 'flag' in plain.lower() or 'the' in plain.lower():
        print(f'Shift {shift}: {plain}')
" "Khoor Zruog"
```

### ROT47
```bash
# ROT47 (full printable ASCII)
python3 -c "
import sys
s = sys.argv[1]
print(''.join(chr((ord(c) - 33 + 47) % 94 + 33) if 33 <= ord(c) <= 126 else c for c in s))
" "Jxy:7@C7"
```

## RSA Quick Attacks

### Small Exponent (e=3)
```python
from Crypto.PublicKey import RSA
import gmpy2

# If c < n and e=3, cube root might work
c = int.from_bytes(ciphertext, 'big')
root, exact = gmpy2.iroot(c, 3)
if exact:
    flag = int(root).to_bytes((int(root).bit_length() + 7) // 8, 'big')
    print(f"Flag: {flag}")
```

### Wiener's Attack (small private exponent d)
```python
from Crypto.PublicKey import RSA
from sympy import continued_fraction, Rational

# If d < n^0.25 / 3, Wiener's attack works
def wiener_attack(e, n):
    cf = continued_fraction(Rational(e, n))
    convergents = cf.convergents()
    for c in convergents:
        k, d = c.p, c.q
        if k == 0:
            continue
        phi = (e * d - 1) // k
        # Check if phi is valid
        b = n - phi + 1
        discriminant = b * b - 4 * n
        if discriminant >= 0:
            sqrt_disc = gmpy2.isqrt(discriminant)
            if sqrt_disc * sqrt_disc == discriminant:
                p = (b + sqrt_disc) // 2
                q = (b - sqrt_disc) // 2
                if p * q == n:
                    return int(d)
    return None
```

### Common Modulus Attack
```python
# If same message encrypted with same n but different e values
from Crypto.PublicKey import RSA
import gmpy2

# Given: (n, e1, c1) and (n, e2, c2)
# If gcd(e1, e2) = 1, we can recover m
def common_modulus(n, e1, c1, e2, c2):
    g, s, t = gmpy2.gcdext(e1, e2)
    if s < 0:
        c1 = gmpy2.invert(c1, n)
        s = -s
    if t < 0:
        c2 = gmpy2.invert(c2, n)
        t = -t
    m = (pow(c1, s, n) * pow(c2, t, n)) % n
    return m
```

### Hastad's Broadcast Attack (same message, multiple e values)
```python
from Crypto.PublicKey import RSA
import gmpy2

# If same message encrypted with different public keys (e=3, multiple n values)
# Use CRT to combine ciphertexts
def hastad_broadcast(moduli, ciphertexts, e=3):
    # CRT
    N = 1
    for n in moduli:
        N *= n
    
    result = 0
    for i, (n_i, c_i) in enumerate(zip(moduli, ciphertexts)):
        N_i = N // n_i
        M_i = gmpy2.invert(N_i, n_i)
        result += c_i * N_i * M_i
    
    result %= N
    # Take e-th root
    root, exact = gmpy2.iroot(result, e)
    return int(root) if exact else None
```

### Bleichenbacher's Attack (PKCS#1 v1.5)
```python
# If server leaks "decryption failed" vs "padding valid" errors
# This is an oracle attack - requires network interaction

def bleichenbacher_oracle(n, e, c, oracle):
    """
    oracle: function that returns True if padding is valid
    Returns decrypted plaintext
    """
    k = (n.bit_length() + 7) // 8
    B = pow(2, 8 * (k - 2))
    c0 = c
    
    # Step 1: Blinding
    s0 = 1
    c0 = (c0 * pow(s0, e, n)) % n
    
    # Step 2: Searching for PKCS-conforming message
    s1 = (n + 3 * B - 1) // (3 * B)
    while True:
        c1 = (c0 * pow(s1, e, n)) % n
        if oracle(c1):
            break
        s1 += 1
    
    # ... (full implementation is complex, use existing tools)
    return plaintext
```

## AES Quick Attacks

### ECB Mode - Byte Flipping
```python
from Crypto.Cipher import AES

# ECB: identical blocks → identical ciphertext
# Attack: flip bytes in block to change plaintext in next block

def ecb_byte_flip(ciphertext, block_size=16, target_block=1, target_pos=0, new_byte=b'A'):
    """
    Flip a byte in block to change plaintext in next block
    """
    blocks = [ciphertext[i:i+block_size] for i in range(0, len(ciphertext), block_size)]
    
    # XOR old byte with new byte
    flip = bytes([b ^ n for b, n in zip(blocks[target_block-1][target_pos:target_pos+1], new_byte)])
    
    # Apply flip
    new_block = blocks[target_block-1][:target_pos] + flip + blocks[target_block-1][target_pos+1:]
    blocks[target_block-1] = new_block
    
    return b''.join(blocks)
```

### CBC Mode - Padding Oracle
```python
# Padding oracle attack - decrypt without key
# Requires server that tells you if padding is valid

def padding_oracle_attack(n, iv, ciphertext, oracle):
    """
    oracle: function that returns True if padding is valid
    Returns decrypted plaintext
    """
    block_size = 16
    blocks = [ciphertext[i:i+block_size] for i in range(0, len(ciphertext), block_size)]
    
    plaintext = b''
    for i, block in enumerate(blocks):
        prev = iv if i == 0 else blocks[i-1]
        decrypted_block = b''
        
        for byte_pos in range(block_size - 1, -1, -1):
            padding_byte = block_size - byte_pos
            
            # Build suffix
            suffix = bytes([b ^ padding_byte for b in decrypted_block[byte_pos+1:]])
            
            for guess in range(256):
                test_byte = bytes([guess])
                # Construct test block
                test_block = b'\x00' * byte_pos + test_byte + suffix
                
                # XOR with previous block to get correct IV
                test_iv = bytes([t ^ p for t, p in zip(test_block, prev)])
                
                if oracle(test_iv + block):
                    # Verify it's not false positive
                    if byte_pos == block_size - 1:
                        # Test with different last byte
                        test_block2 = b'\x00' * byte_pos + bytes([guess ^ 1]) + suffix
                        test_iv2 = bytes([t ^ p for t, p in zip(test_block2, prev)])
                        if not oracle(test_iv2 + block):
                            continue
                    
                    decrypted_block = bytes([guess ^ padding_byte]) + decrypted_block
                    break
        
        plaintext += decrypted_block
    
    return plaintext
```

### CBC Mode - IV Reuse
```python
# If same IV used for two messages, XOR ciphertexts to get XOR of plaintexts
def cbc_iv_reuse(c1, c2, block_size=16):
    """XOR two ciphertexts to get XOR of plaintexts"""
    blocks1 = [c1[i:i+block_size] for i in range(0, len(c1), block_size)]
    blocks2 = [c2[i:i+block_size] for i in range(0, len(c2), block_size)]
    
    xor_result = b''
    for b1, b2 in zip(blocks1, blocks2):
        xor_result += bytes([a ^ b for a, b in zip(b1, b2)])
    
    return xor_result
```

## Frequency Analysis

### Single-Character Frequency Analysis
```python
from collections import Counter

ENGLISH_FREQ = {
    'a': 8.2, 'b': 1.5, 'c': 2.8, 'd': 4.3, 'e': 13.0, 'f': 2.2,
    'g': 2.0, 'h': 6.1, 'i': 7.0, 'j': 0.15, 'k': 0.77, 'l': 4.0,
    'm': 2.4, 'n': 6.7, 'o': 7.5, 'p': 1.9, 'q': 0.095, 'r': 6.0,
    's': 6.3, 't': 9.1, 'u': 2.8, 'v': 0.98, 'w': 2.4, 'x': 0.15,
    'y': 2.0, 'z': 0.074
}

def frequency_analysis(ciphertext):
    """Find likely shift/key using frequency analysis"""
    freq = Counter(ciphertext.lower())
    n = len(ciphertext)
    
    # Calculate chi-squared score for each possible shift
    scores = []
    for shift in range(26):
        score = 0
        for char in 'abcdefghijklmnopqrstuvwxyz':
            expected = ENGLISH_FREQ[char] * n / 100
            observed = freq.get(chr((ord(char) - ord('a') + shift) % 26 + ord('a')), 0)
            if expected > 0:
                score += (observed - expected) ** 2 / expected
        scores.append((shift, score))
    
    scores.sort(key=lambda x: x[1])
    return scores[0][0]  # Most likely shift
```

### Vigenere Cipher Breaking
```python
def vigenere_length(ciphertext):
    """Find likely key length using index of coincidence"""
    def ic(text):
        freq = [text.count(chr(i)) for i in range(ord('a'), ord('z') + 1)]
        n = len(text)
        return sum(f * (f - 1) for f in freq) / (n * (n - 1)) if n > 1 else 0
    
    # Try different key lengths
    scores = []
    for kl in range(1, 50):
        groups = [ciphertext[i::kl] for i in range(kl)]
        avg_ic = sum(ic(g) for g in groups) / kl
        scores.append((kl, avg_ic))
    
    # English IC ≈ 0.067
    scores.sort(key=lambda x: -x[1])
    return scores[0][0]

def vigenere_crack(ciphertext, key_length):
    """Crack Vigenere with known key length"""
    key = ''
    for i in range(key_length):
        group = ciphertext[i::key_length]
        shift = frequency_analysis(group)
        key += chr(shift + ord('a'))
    return key
```

## Hash Attacks

### MD5/SHA1 Collision
```bash
# Create MD5 collision (fastcoll)
fastcoll -p prefix.txt -o out1.bin out2.bin

# SHA1 collision (shattered)
# Download shattered-1.pdf and shattered-2.pdf (same SHA1, different content)
```

### Length Extension Attack
```python
import hashpumpy

# If server uses: hash(secret + message)
# We can compute: hash(secret + message + padding + extension)
# WITHOUT knowing the secret

def length_extension(original_hash, original_message, append_message, secret_length):
    new_hash, new_message = hashpumpy.hashpump(
        original_hash,
        original_message,
        append_message,
        secret_length
    )
    return new_hash, new_message
```

### Hashcat Quick Cracks
```bash
# MD5
hashcat -m 0 hash.txt /usr/share/wordlists/rockyou.txt

# SHA1
hashcat -m 100 hash.txt /usr/share/wordlists/rockyou.txt

# SHA256
hashcat -m 1400 hash.txt /usr/share/wordlists/rockyou.txt

# bcrypt
hashcat -m 3200 hash.txt /usr/share/wordlists/rockyou.txt

# NTLM (Windows)
hashcat -m 1000 hash.txt /usr/share/wordlists/rockyou.txt

# With rules
hashcat -m 0 hash.txt /usr/share/wordlists/rockyou.txt -r /usr/share/hashcat/rules/best64.rule
```

## Crypto CTF Pattern Database

### Pattern: XOR with Repeating Key
```
Detection: Multiple ciphertexts of similar length
Attack:
1. Try short keys (1-8 bytes) with brute force
2. If two plaintexts XORed together → key cancels out
3. Use Kasiski examination for key length
```

### Pattern: RSA with Small e (e=3)
```
Detection: Public exponent is 3
Attack:
1. Try cube root (gmpy2.iroot(c, 3))
2. If multiple messages, use Hastad's broadcast attack
3. If c > n, padding might be incomplete
```

### Pattern: AES-ECB
```
Detection: Ciphertext length is multiple of 16, same plaintext → same ciphertext
Attack:
1. Byte flipping (modify ciphertext to change plaintext)
2. ECB oracle (determine block boundaries)
3. Cut-and-paste (reorder blocks)
```

### Pattern: Substitution Cipher
```
Detection: Letter frequency matches English but letters are swapped
Attack:
1. Frequency analysis
2. Known plaintext (guess common words like "the", "flag")
3. Online tools (quipqiup.com)
```

### Pattern: Custom Stream Cipher
```
Detection: XOR with pseudo-random stream
Attack:
1. If key reused: XOR ciphertexts → XOR of plaintexts
2. If LCG: recover state from outputs
3. If weak PRNG: predict next outputs
```

## Speed Metrics
```
Average solve times (target):
- Base64/Hex/ROT13: <1 minute
- Single-byte XOR: <2 minutes
- Caesar cipher: <2 minutes
- RSA small exponent: <3 minutes
- Vigenere: <5 minutes
- AES-ECB byte flip: <5 minutes
- Padding oracle: <10 minutes
- Complex custom crypto: <15 minutes
```
