import struct, zlib
import numpy as np
import time
import hashlib

TARGET = bytes.fromhex("ff2142e98e09b5344994f9beb9c56c95506b9f17")

with open('puzzle_image.png', 'rb') as f:
    raw = f.read()
pos = 8
idat_raw = b''
while pos < len(raw):
    length = struct.unpack('>I', raw[pos:pos+4])[0]
    ct = raw[pos+4:pos+8]
    cd = raw[pos+8:pos+8+length]
    if ct == b'IDAT': idat_raw += cd
    pos += 12 + length
decompressed = zlib.decompress(idat_raw)

def undo_filter(ft, rd, pr, bpp=2):
    r = bytearray(len(rd))
    if ft == 0: r[:] = rd
    elif ft == 1:
        for i in range(len(rd)):
            r[i] = (rd[i] + (r[i-bpp] if i>=bpp else 0)) & 0xFF
    elif ft == 2:
        for i in range(len(rd)):
            r[i] = (rd[i] + (pr[i] if pr else 0)) & 0xFF
    elif ft == 4:
        for i in range(len(rd)):
            l = r[i-bpp] if i>=bpp else 0
            u = pr[i] if pr else 0
            ul = pr[i-bpp] if pr and i>=bpp else 0
            p = l+u-ul
            pa,pb,pc = abs(p-l),abs(p-u),abs(p-ul)
            pred = l if (pa<=pb and pa<=pc) else (u if pb<=pc else ul)
            r[i] = (rd[i] + pred) & 0xFF
    return bytes(r)

print("Decoding pixels...")
rows_data = []
pr = None
for ri in range(1105):
    s = ri * (1 + 1600*2)
    ft = decompressed[s]
    rd = decompressed[s+1:s+1+1600*2]
    rows_data.append(undo_filter(ft, rd, pr))
    pr = rows_data[-1]

gray = np.zeros((1105,1600), dtype=np.uint8)
alpha = np.zeros((1105,1600), dtype=np.uint8)
for ri, rd in enumerate(rows_data):
    for c in range(1600):
        gray[ri,c] = rd[c*2]
        alpha[ri,c] = rd[c*2+1]

print("Pixels decoded.")

# Try to import faster EC library
try:
    from fastecdsa.curve import P256
    from fastecdsa.point import Point
    from fastecdsa import keys as fecdsa_keys
    HAS_FASTECDSA = True
    print("Using fastecdsa")
except ImportError:
    try:
        from coincurve import PrivateKey
        HAS_COINCURVE = True
        print("Using coincurve")
    except ImportError:
        HAS_COINCURVE = False
        HAS_FASTECDSA = False
        print("Using pure python EC (slow)")

# secp256k1 parameters
P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
Gx = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
Gy = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8

def modinv(a, m=P):
    if a < 0: a = a % m
    g, x, _ = extended_gcd(a, m)
    return x % m

def extended_gcd(a, b):
    if a == 0: return b, 0, 1
    g, x, y = extended_gcd(b % a, a)
    return g, y - (b // a) * x, x

def point_add(p1, p2):
    if p1 is None: return p2
    if p2 is None: return p1
    if p1[0] == p2[0] and p1[1] != p2[1]: return None
    if p1 == p2:
        lam = (3 * p1[0] * p1[0] * modinv(2 * p1[1], P)) % P
    else:
        lam = ((p2[1] - p1[1]) * modinv((p2[0] - p1[0]) % P, P)) % P
    x = (lam * lam - p1[0] - p2[0]) % P
    y = (lam * (p1[0] - x) - p1[1]) % P
    return (x, y)

def scalar_mult(k, point=None):
    if point is None: point = (Gx, Gy)
    result = None
    addend = point
    while k > 0:
        if k & 1:
            result = point_add(result, addend)
        addend = point_add(addend, addend)
        k >>= 1
    return result

def keccak256(data):
    import sha3
    return sha3.keccak_256(data).digest()

def check_key(key_bytes):
    key_int = int.from_bytes(key_bytes, 'big')
    if key_int == 0 or key_int >= N: return False
    try:
        pub = scalar_mult(key_int)
        if pub is None: return False
        # Uncompressed public key: 04 + x + y
        pub_bytes = b'\x04' + pub[0].to_bytes(32, 'big') + pub[1].to_bytes(32, 'big')
        addr = keccak256(pub_bytes)[-20:]
        return addr == TARGET
    except:
        return False

def check_key_coincurve(key_bytes):
    pk = PrivateKey(key_bytes)
    pub = pk.public_key
    pub_bytes = pub.format(compressed=False)
    addr = keccak256(pub_bytes)[-20:]
    return addr == TARGET

check_fn = check_key_coincurve if HAS_COINCURVE else check_key

# === APPROACH 1: 1-bit LSB of grayscale, raster order ===
print("\n=== APPROACH 1: 1-bit LSB grayscale, raster ===")
gray_flat = gray.flatten()
lsb = (gray_flat & 1).astype(np.uint8)
n_bytes = len(lsb) // 8
packed = np.zeros(n_bytes, dtype=np.uint8)
for i in range(8):
    packed |= (lsb[i::8].astype(np.uint8) << (7 - i))

start = time.time()
checked = 0
found = False
for offset in range(0, n_bytes - 32):
    candidate = bytes(packed[offset:offset+32])
    checked += 1
    if checked % 50000 == 0:
        print(f"  {checked} checked, {time.time()-start:.1f}s")
    if check_fn(candidate):
        print(f"\n*** SOLVED! offset={offset}, key={candidate.hex()}")
        found = True
        break
print(f"Done: {checked} in {time.time()-start:.1f}s")

if not found:
    # === APPROACH 2: 1-bit LSB of grayscale, reverse raster ===
    print("\n=== APPROACH 2: 1-bit LSB grayscale, reverse raster ===")
    lsb_rev = lsb[::-1]
    packed_rev = np.zeros(n_bytes, dtype=np.uint8)
    for i in range(8):
        packed_rev |= (lsb_rev[i::8].astype(np.uint8) << (7 - i))
    
    start = time.time()
    for offset in range(0, n_bytes - 32):
        candidate = bytes(packed_rev[offset:offset+32])
        if check_fn(candidate):
            print(f"\n*** SOLVED! reverse raster offset={offset}, key={candidate.hex()}")
            found = True
            break
        if (offset+1) % 50000 == 0:
            print(f"  {offset+1} checked")
    print(f"Done: {time.time()-start:.1f}s")

if not found:
    # === APPROACH 3: 1-bit LSB of grayscale, column-major ===
    print("\n=== APPROACH 3: 1-bit LSB grayscale, column-major ===")
    gray_col = gray.T.flatten()
    lsb_col = (gray_col & 1).astype(np.uint8)
    packed_col = np.zeros(len(lsb_col)//8, dtype=np.uint8)
    for i in range(8):
        packed_col |= (lsb_col[i::8].astype(np.uint8) << (7 - i))
    
    n_col = len(packed_col)
    start = time.time()
    for offset in range(0, n_col - 32):
        candidate = bytes(packed_col[offset:offset+32])
        if check_fn(candidate):
            print(f"\n*** SOLVED! column-major offset={offset}, key={candidate.hex()}")
            found = True
            break
        if (offset+1) % 50000 == 0:
            print(f"  {offset+1} checked")
    print(f"Done: {time.time()-start:.1f}s")

if not found:
    # === APPROACH 4: 1-bit LSB of alpha ===
    print("\n=== APPROACH 4: 1-bit LSB alpha, raster ===")
    alpha_flat = alpha.flatten()
    lsb_a = (alpha_flat & 1).astype(np.uint8)
    n_a = len(lsb_a) // 8
    packed_a = np.zeros(n_a, dtype=np.uint8)
    for i in range(8):
        packed_a |= (lsb_a[i::8].astype(np.uint8) << (7 - i))
    
    start = time.time()
    for offset in range(0, n_a - 32):
        candidate = bytes(packed_a[offset:offset+32])
        if check_fn(candidate):
            print(f"\n*** SOLVED! alpha LSB offset={offset}, key={candidate.hex()}")
            found = True
            break
        if (offset+1) % 50000 == 0:
            print(f"  {offset+1} checked")
    print(f"Done: {time.time()-start:.1f}s")

if not found:
    # === APPROACH 5: 2-bit LSB of grayscale, raster ===
    print("\n=== APPROACH 5: 2-bit LSB grayscale, raster ===")
    bits2 = (gray_flat & 3).astype(np.uint8)
    n2 = len(bits2) // 4
    packed2 = np.zeros(n2, dtype=np.uint8)
    for i in range(4):
        packed2 |= (bits2[i::4].astype(np.uint8) << (6 - 2*i))
    
    start = time.time()
    for offset in range(0, n2 - 32):
        candidate = bytes(packed2[offset:offset+32])
        if check_fn(candidate):
            print(f"\n*** SOLVED! 2-bit LSB offset={offset}, key={candidate.hex()}")
            found = True
            break
        if (offset+1) % 50000 == 0:
            print(f"  {offset+1} checked")
    print(f"Done: {time.time()-start:.1f}s")

if not found:
    # === APPROACH 6: 4-bit LSB of grayscale, raster ===
    print("\n=== APPROACH 6: 4-bit LSB grayscale, raster ===")
    bits4 = (gray_flat & 15).astype(np.uint8)
    n4 = len(bits4) // 2
    packed4 = np.zeros(n4, dtype=np.uint8)
    for i in range(2):
        packed4 |= (bits4[i::2].astype(np.uint8) << (4 - 4*i))
    
    start = time.time()
    for offset in range(0, n4 - 32):
        candidate = bytes(packed4[offset:offset+32])
        if check_fn(candidate):
            print(f"\n*** SOLVED! 4-bit LSB offset={offset}, key={candidate.hex()}")
            found = True
            break
        if (offset+1) % 50000 == 0:
            print(f"  {offset+1} checked")
    print(f"Done: {time.time()-start:.1f}s")

if not found:
    # === APPROACH 7: 1-bit MSB of grayscale (bit 7), raster ===
    print("\n=== APPROACH 7: 1-bit MSB grayscale (bit 7), raster ===")
    msb7 = ((gray_flat >> 7) & 1).astype(np.uint8)
    n7 = len(msb7) // 8
    packed7 = np.zeros(n7, dtype=np.uint8)
    for i in range(8):
        packed7 |= (msb7[i::8].astype(np.uint8) << (7 - i))
    
    start = time.time()
    for offset in range(0, n7 - 32):
        candidate = bytes(packed7[offset:offset+32])
        if check_fn(candidate):
            print(f"\n*** SOLVED! MSB7 offset={offset}, key={candidate.hex()}")
            found = True
            break
        if (offset+1) % 50000 == 0:
            print(f"  {offset+1} checked")
    print(f"Done: {time.time()-start:.1f}s")

if not found:
    # === APPROACH 8: bit 1 of grayscale (second LSB), raster ===
    print("\n=== APPROACH 8: bit 1 grayscale, raster ===")
    bit1 = ((gray_flat >> 1) & 1).astype(np.uint8)
    n1 = len(bit1) // 8
    packed1 = np.zeros(n1, dtype=np.uint8)
    for i in range(8):
        packed1 |= (bit1[i::8].astype(np.uint8) << (7 - i))
    
    start = time.time()
    for offset in range(0, n1 - 32):
        candidate = bytes(packed1[offset:offset+32])
        if check_fn(candidate):
            print(f"\n*** SOLVED! bit1 offset={offset}, key={candidate.hex()}")
            found = True
            break
        if (offset+1) % 50000 == 0:
            print(f"  {offset+1} checked")
    print(f"Done: {time.time()-start:.1f}s")

if not found:
    # === APPROACH 9: gray channel, each pixel value modulo 2, interleaved with alpha ===
    print("\n=== APPROACH 9: gray+alpha interleaved LSB ===")
    combined = np.zeros(total * 2, dtype=np.uint8)
    combined[0::2] = gray_flat & 1
    combined[1::2] = alpha_flat & 1
    nc = len(combined) // 8
    packed_c = np.zeros(nc, dtype=np.uint8)
    for i in range(8):
        packed_c |= (combined[i::8].astype(np.uint8) << (7-i))
    
    start = time.time()
    for offset in range(0, nc - 32):
        candidate = bytes(packed_c[offset:offset+32])
        if check_fn(candidate):
            print(f"\n*** SOLVED! interleaved offset={offset}, key={candidate.hex()}")
            found = True
            break
        if (offset+1) % 50000 == 0:
            print(f"  {offset+1} checked")
    print(f"Done: {time.time()-start:.1f}s")

print(f"\nAll approaches done. Found: {found}")
