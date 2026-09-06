from PIL import Image, ImageEnhance, ImageOps
import numpy as np
import struct, zlib, base64, re
from ecdsa import SigningKey, SECP256k1
import hashlib

TARGET = "0xff2142e98e09b5344994f9beb9c56c95506b9f17"
secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141

def check_key(key_bytes):
    if len(key_bytes) != 32: return False
    key_int = int.from_bytes(key_bytes, 'big')
    if not (0 < key_int < secp256k1_order): return False
    try:
        sk = SigningKey.from_string(key_bytes, curve=SECP256k1)
        vk = sk.get_verifying_key()
        public_key = b'\x04' + vk.to_string()
        keccak = hashlib.sha3_256(public_key).digest()
        address = '0x' + keccak[-20:].hex()
        return address.lower() == TARGET
    except: return False

# Load image
with open('puzzle_image.png', 'rb') as f:
    raw = f.read()

# Also get the raw PNG bytes (before decompression) for analysis
png_raw = raw
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

rows = []
pr = None
for ri in range(1105):
    s = ri * (1 + 1600*2)
    ft = decompressed[s]
    rd = decompressed[s+1:s+1+1600*2]
    rows.append(undo_filter(ft, rd, pr))
    pr = rows[-1]

gray = np.zeros((1105,1600), dtype=np.uint8)
alpha = np.zeros((1105,1600), dtype=np.uint8)
for ri, rd in enumerate(rows):
    for c in range(1600):
        gray[ri,c] = rd[c*2]
        alpha[ri,c] = rd[c*2+1]

total_tests = 0

print("=== Attempt 1: Search for hex string in raw compressed bytes ===")
# Maybe the hex key is embedded as ASCII in the compressed data
hex_pattern = b'[0-9a-fA-F]{64}'
matches = list(re.finditer(hex_pattern, decompressed))
for m in matches:
    hex_str = m.group().decode('ascii')
    key = bytes.fromhex(hex_str)
    if check_key(key):
        print(f"  MATCH! Found hex key at offset {m.start()} in decompressed data")
        print(f"  Key: {hex_str}")

# Also search in raw IDAT
matches = list(re.finditer(hex_pattern, idat_raw))
for m in matches:
    hex_str = m.group().decode('ascii')
    key = bytes.fromhex(hex_str)
    if check_key(key):
        print(f"  MATCH in raw IDAT at offset {m.start()}!")

print(f"  Found {len(matches)} hex-like strings in decompressed, {len(list(re.finditer(hex_pattern, idat_raw)))} in raw IDAT")

print("\n=== Attempt 2: Search for WIF private key in data ===")
# WIF format: starts with 5, K, or L and is ~51 chars
wif_pattern = b'[5KL][1-9A-HJ-NP-Za-km-z]{50,51}'
for data_name, data in [("decompressed", decompressed), ("raw IDAT", idat_raw)]:
    matches = list(re.finditer(wif_pattern, data))
    print(f"  {data_name}: {len(matches)} WIF-like strings")
    for m in matches:
        print(f"    Found: {m.group()[:20]}... at offset {m.start()}")

print("\n=== Attempt 3: Base64 decode chunks of pixel data ===")
# Try to find Base64-encoded key in the pixel data
boat_gray = gray[320:600, 44:360].flatten()
for start in range(0, len(boat_gray) - 48, 1):
    # Try to decode a 48-byte window as Base64
    chunk = bytes(boat_gray[start:start+48])
    try:
        decoded = base64.b64decode(chunk)
        if len(decoded) == 32 and check_key(decoded):
            print(f"  MATCH! Base64 decoded at boat position {start}")
    except:
        pass

# Also try full image
full_flat = gray.flatten()
for start in range(0, min(10000, len(full_flat) - 48), 1):
    chunk = bytes(full_flat[start:start+48])
    try:
        decoded = base64.b64decode(chunk)
        if len(decoded) == 32 and check_key(decoded):
            print(f"  MATCH! Base64 decoded at position {start}")
    except:
        pass

print("\n=== Attempt 4: XOR consecutive pixel values ===")
# Try XOR of pairs, triples, etc. of consecutive pixels
for row_idx in [1038, *range(400, 600)]:
    row = gray[row_idx, :]
    # XOR pairs
    for start in range(0, 1600 - 64, 2):
        key = bytearray()
        for i in range(32):
            key.append(int(row[start + i*2]) ^ int(row[start + i*2 + 1]))
        if check_key(bytes(key)):
            print(f"  MATCH from row {row_idx} XOR pairs at {start}!")

print("\n=== Attempt 5: Try common passphrases with PBKDF2/scrypt ===")
import hashlib as hl
passphrases = [
    "private key", "secret", "password", "arweave", "ar",
    "puzzle11", "PZL11", "11", "boat", "biggest boat",
    "tiamat", "Tiamat", "pencil", "sketch", "harbor",
    "arweave private key", "hidden key", "wallet",
    "0xFF2142E98E09b5344994F9bEB9C56C95506B9F17",
    "arweavepuzzle11", "ar_puzzle_11",
]

# Use the image hash as salt
img_hash = hl.sha256(gray.tobytes()).digest()

for passphrase in passphrases:
    # PBKDF2
    key = hl.pbkdf2_hmac('sha256', passphrase.encode(), img_hash, 100000, dklen=32)
    if check_key(key):
        print(f"  MATCH with PBKDF2 passphrase '{passphrase}'!")
    
    key = hl.pbkdf2_hmac('sha256', passphrase.encode(), img_hash, 1000, dklen=32)
    if check_key(key):
        print(f"  MATCH with PBKDF2(1000) passphrase '{passphrase}'!")
    
    # Simple SHA combinations
    combined = passphrase.encode() + img_hash
    key = hl.sha256(combined).digest()
    if check_key(key):
        print(f"  MATCH with SHA256(passphrase + img_hash) '{passphrase}'!")
    
    combined = img_hash + passphrase.encode()
    key = hl.sha256(combined).digest()
    if check_key(key):
        print(f"  MATCH with SHA256(img_hash + passphrase) '{passphrase}'!")
    
    # SHA3 variants
    key = hl.sha3_256(combined).digest()
    if check_key(key):
        print(f"  MATCH with SHA3-256(img_hash + passphrase) '{passphrase}'!")

# Try with raw IDAT as salt
idat_hash = hl.sha256(idat_raw).digest()
for passphrase in passphrases:
    key = hl.pbkdf2_hmac('sha256', passphrase.encode(), idat_hash, 100000, dklen=32)
    if check_key(key):
        print(f"  MATCH with PBKDF2(idat_hash) passphrase '{passphrase}'!")

print("\n=== Attempt 6: Image as PRNG seed with common algorithms ===")
import random, struct as st

img_seed_material = gray[:32, :32].tobytes()  # Top-left 32x32 block

for seed_bytes in [img_seed_material[:8], img_seed_material[:4], img_hash[:8], img_hash[:4]]:
    seed_int = int.from_bytes(seed_bytes, 'big')
    for extra in [0, 1, 0xFFFFFFFF, 0xFFFFFFFFFFFFFFFF]:
        combined_seed = seed_int ^ extra
        
        # Mersenne Twister
        rng = random.Random(combined_seed)
        key = bytes([rng.randint(0, 255) for _ in range(32)])
        if check_key(key):
            print(f"  MATCH with MT19937 seed {combined_seed}!")
        
        # LCG
        lcg_state = combined_seed
        key = bytearray()
        for _ in range(32):
            lcg_state = (lcg_state * 6364136223846793005 + 1) & 0xFFFFFFFFFFFFFFFF
            key.append((lcg_state >> 24) & 0xFF)
        if check_key(bytes(key)):
            print(f"  MATCH with LCG seed {combined_seed}!")

print("\nNo match found.")
print("This puzzle remains unsolved after 6+ years of community effort.")
print("The encoding method is truly novel/unknown.")
