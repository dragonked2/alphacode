from PIL import Image
import numpy as np
import struct, zlib
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

# The key insight: "PK is on the biggest boat"
# Let's focus EXCLUSIVELY on the boat region
# Sailboat bounding box: x:44-359, y:320-599
# But there might be a smaller "biggest boat" elsewhere

print("=== Attempt: First 32 unique grayscale values in boat region ===")
boat = gray[320:600, 44:360]
seen = set()
unique_vals = []
for row in boat:
    for v in row:
        v = int(v)
        if v not in seen:
            seen.add(v)
            unique_vals.append(v)

print(f"  Unique grayscale values in boat: {len(unique_vals)}")
# Take first 32
key = bytes(unique_vals[:32])
if check_key(key):
    print(f"  MATCH! First 32 unique values!")
# Reverse
key = bytes(unique_vals[:32][::-1])
if check_key(key):
    print(f"  MATCH! First 32 unique values reversed!")

# Filter: only values that appear exactly once in the boat
from collections import Counter
boat_flat = [int(v) for v in boat.flatten()]
counts = Counter(boat_flat)
unique_once = [v for v, c in counts.items() if c == 1]
print(f"  Values appearing exactly once in boat: {len(unique_once)}")
key = bytes(unique_once[:32])
if check_key(key):
    print(f"  MATCH! Unique-once values!")
key = bytes(unique_once[-32:])
if check_key(key):
    print(f"  MATCH! Last 32 unique-once values!")

print("\n=== Attempt: Boat region pixel values read as ASCII ===")
# Maybe text is written in pixel values in the boat area
boat_text = ''.join(chr(int(v)) if 32 <= int(v) <= 126 else '.' for v in boat.flatten())
# Search for patterns
for pattern in ['0x', 'ff', 'FF', 'key', 'KEY', 'priv', 'wallet', 'arweave', '0xFF']:
    idx = boat_text.find(pattern)
    if idx >= 0:
        context = boat_text[max(0,idx-5):idx+37]
        print(f"  Found '{pattern}' in boat ASCII at offset {idx}: {repr(context)}")

print("\n=== Attempt: Reading alpha-channel-only boat pixels ===")
alpha_boat = alpha[320:600, 44:360]
# Only pixels where alpha != 255 (the anti-aliasing values)
aa_pixels = []
for y in range(alpha_boat.shape[0]):
    for x in range(alpha_boat.shape[1]):
        a = int(alpha_boat[y, x])
        if a != 255:
            aa_pixels.append(a)

print(f"  Anti-aliasing pixels in boat: {len(aa_pixels)}")
if len(aa_pixels) >= 32:
    key = bytes(aa_pixels[:32])
    if check_key(key):
        print(f"  MATCH! First 32 AA values!")
    key = bytes(aa_pixels[-32:])
    if check_key(key):
        print(f"  MATCH! Last 32 AA values!")

print("\n=== Attempt: Large-scale pixel value histogram as key ===")
# Maybe the histogram encodes the key
hist, bins = np.histogram(gray, bins=256, range=(0,255))
# Try different subsets
for start in range(0, 256, 1):
    end = start + 32
    if end <= 256:
        key = bytes(hist[start:end].astype(int) % 256)
        if check_key(key):
            print(f"  MATCH from histogram bins {start}-{end}!")

print("\n=== Attempt: Raw decompressed byte stream LSB of entire image ===")
# Maybe the key is encoded in the raw decompressed stream differently
# Try reading every Nth byte from the decompressed data
for stride in range(2, 100):
    extracted = decompressed[::stride]
    # Test first 32 bytes
    if len(extracted) >= 32:
        key = bytes(extracted[:32])
        if check_key(key):
            print(f"  MATCH from decompressed stride {stride}!")
        key = bytes(extracted[-32:])
        if check_key(key):
            print(f"  MATCH from decompressed stride {stride} (last 32)!")

print("\n=== Attempt: PNG file structure bytes as key ===")
# Read raw PNG bytes (excluding signature and IDAT)
all_non_idat = b''
pos = 8
while pos < len(raw):
    length = struct.unpack('>I', raw[pos:pos+4])[0]
    ct = raw[pos+4:pos+8]
    cd = raw[pos+8:pos+8+length]
    if ct != b'IDAT':
        all_non_idat += cd
    pos += 12 + length

print(f"  Non-IDAT chunk data: {len(all_non_idat)} bytes")
# Try different subsets as key
for start in range(0, max(1, len(all_non_idat) - 32), 1):
    key = all_non_idat[start:start+32]
    if len(key) == 32 and check_key(key):
        print(f"  MATCH from non-IDAT bytes at offset {start}!")

# Try hash of all non-IDAT data
import hashlib as hl
key = hl.sha256(all_non_idat).digest()
if check_key(key):
    print(f"  MATCH from SHA256 of non-IDAT data!")
key = hl.sha256(all_non_idat + b'\x00').digest()
if check_key(key):
    print(f"  MATCH from SHA256(non-IDAT + null)!")

print("\n=== Attempt: Interpreting pixel grid as BIP39 mnemonic ===")
# 12 words could come from building heights
# But BIP39 12 words = 128 bits, not 256 bits (private key)
# Unless it's 24 words... 24 words = 256 bits!
# We only have 12 buildings. Could there be 24 measurements?
# 12 heights + 12 widths = 24 values!
building_heights = [100, 190, 90, 165, 240, 55, 180, 160, 90, 160, 100, 190]
building_widths = [115, 90, 100, 170, 100, 100, 150, 110, 110, 100, 70, 50]

# 24 BIP39 words would need 24 indices, each 0-2047
all_dims = building_heights + building_widths
print(f"  24 values (heights+widths): {all_dims}")
print(f"  All in BIP39 range (0-2047)? {all(0 <= v < 2048 for v in all_dims)}")

# To convert 24 BIP39 word indices to a 256-bit key:
# Each word index is 11 bits, 24*11 = 264 bits
# First 256 bits = private key, last 8 bits = checksum
bits = 0
for v in all_dims:
    bits = (bits << 11) | v
# Convert to bytes
key_bytes = bits.to_bytes(33, byteorder='big')[:32]
print(f"  Derived key (heights+widths as BIP39): {key_bytes.hex()}")
if check_key(key_bytes):
    print(f"  MATCH! heights+widths as BIP39 indices!")

# Try all 24 in different order
from itertools import permutations
# Too many permutations for 24 items, but try reversed
reversed_dims = list(reversed(all_dims))
bits = 0
for v in reversed_dims:
    bits = (bits << 11) | v
key_bytes = bits.to_bytes(33, byteorder='big')[:32]
if check_key(key_bytes):
    print(f"  MATCH! Reversed heights+widths as BIP39 indices!")

# Try heights only (12 words = 128 bits) used as SHA seed
# 12 words can't directly be a 256-bit key, but could be hashed
for hash_func in [hl.sha256, hl.sha3_256, hl.sha384, hl.sha512]:
    word_str = ' '.join(str(v) for v in building_heights)
    key = hash_func(word_str.encode()).digest()[:32]
    if check_key(key):
        print(f"  MATCH from heights as word string + {hash_func.__name__}!")
    
    word_str = ' '.join(str(v) for v in all_dims)
    key = hash_func(word_str.encode()).digest()[:32]
    if check_key(key):
        print(f"  MATCH from all dims as word string + {hash_func.__name__}!")

print("\n=== Attempt: Image metadata string hashing ===")
# Try hashing the tEXt comment with different things
comment = "0xFF2142E98E09b5344994F9bEB9C56C95506B9F17"
for hash_func in [hl.sha256, hl.sha3_256, hl.sha512]:
    key = hash_func(comment.encode()).digest()
    if check_key(key):
        print(f"  MATCH from SHA of address! {hash_func.__name__}")
    
    # Address without 0x
    addr = comment[2:]
    key = hash_func(addr.encode()).digest()
    if check_key(key):
        print(f"  MATCH from SHA of address (no 0x)! {hash_func.__name__}")
    
    # Lowercase
    key = hash_func(comment.lower().encode()).digest()
    if check_key(key):
        print(f"  MATCH from SHA of lowercase address!")
    
    # Try address + image hash combinations
    img_h = hl.sha256(gray.tobytes()).digest()
    key = hash_func(comment.encode() + img_h).digest()
    if check_key(key):
        print(f"  MATCH from SHA(address + img_hash)!")

print("\n=== FINAL: Try interpreting different pixel regions as raw 32-byte keys ===")
# Systematically try every contiguous block of 32 pixels
# in specific regions of the image
for name, y1, y2, x1, x2 in [
    ("boat", 320, 600, 44, 360),
    ("skyline", 18, 320, 0, 400),
    ("water", 800, 1105, 0, 1600),
    ("top_32rows", 0, 32, 0, 1600),
    ("left_32cols", 0, 1105, 0, 32),
]:
    region = gray[y1:y2, x1:x2]
    flat = region.flatten()
    
    # Try every contiguous 32-byte window (with step for speed)
    for start in range(0, len(flat) - 32, max(1, len(flat) // 50000)):
        key = flat[start:start+32].tobytes()
        if check_key(key):
            print(f"  MATCH! {name} region offset {start}")
    
    # Try reading column-by-column
    for col in range(region.shape[1]):
        col_data = region[:, col].flatten()
        if len(col_data) >= 32:
            key = col_data[:32].tobytes()
            if check_key(key):
                print(f"  MATCH! {name} column {col}")

print("\n=== Try: grayscale values as BINARY representation ===")
# What if each pixel's value (0 or 1) encodes a bit?
# Threshold the image and read as binary
for thresh in range(10, 250, 10):
    binary = (gray > thresh).astype(np.uint8).flatten()
    # Try every starting position
    for start in range(0, len(binary) - 256, 256):
        key_bytes = bytearray()
        for i in range(32):
            byte_val = 0
            for j in range(8):
                byte_val = (byte_val << 1) | int(binary[start + i*8 + j])
            key_bytes.append(byte_val)
        if check_key(bytes(key_bytes)):
            print(f"  MATCH from binary threshold {thresh}, offset {start}!")

print("\nAll attempts exhausted. Puzzle remains unsolved.")
