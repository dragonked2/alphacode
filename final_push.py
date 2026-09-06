from PIL import Image
import numpy as np
import struct, zlib, hmac, hashlib as hl
from ecdsa import SigningKey, SECP256k1
import hashlib

TARGET = "0xff2142e98e09b5344994f9beb9c56c95506b9f17"
secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141

def check_key(key_bytes):
    if len(key_bytes) != 32: return False, None
    key_int = int.from_bytes(key_bytes, 'big')
    if not (0 < key_int < secp256k1_order): return False, None
    try:
        sk = SigningKey.from_string(key_bytes, curve=SECP256k1)
        vk = sk.get_verifying_key()
        public_key = b'\x04' + vk.to_string()
        keccak = hashlib.sha3_256(public_key).digest()
        address = '0x' + keccak[-20:].hex()
        return address.lower() == TARGET, address
    except: return False, None

def check_and_report(key, label):
    global total_tests
    total_tests += 1
    match, addr = check_key(key)
    if match:
        print(f"\n*** SOLVED! ***")
        print(f"  Key: {key.hex()}")
        print(f"  Address: {addr}")
        print(f"  Method: {label}")
        exit(0)

total_tests = 0

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

# CRITICAL: Let me look at the RAW decompressed data before filtering
# The filter bytes modify the pixel data. Before filtering, the data is different.
# What if the key is in the PRE-FILTER data?

print("=== APPROACH: Pre-filter data analysis ===")
# The decompressed data has 1105 rows, each with 1 filter byte + 3200 data bytes
# Let's look at the RAW data bytes (before filter reversal)
pre_filter_data = bytearray()
filter_bytes = []
for ri in range(1105):
    s = ri * (1 + 1600*2)
    ft = decompressed[s]
    filter_bytes.append(ft)
    row_data = decompressed[s+1:s+1+1600*2]
    pre_filter_data.extend(row_data)

pre_filter = bytes(pre_filter_data)
print(f"  Pre-filter data size: {len(pre_filter)} bytes")

# Try every 32-byte window in the pre-filter data
# But only test a sample for speed
step = max(1, len(pre_filter) // 1000000)
for start in range(0, len(pre_filter) - 32, step):
    key = pre_filter[start:start+32]
    check_and_report(key, f"Pre-filter data offset {start}")

print(f"  Pre-filter done. Tests: {total_tests}")

print("\n=== APPROACH: HMAC-based key derivation ===")
img_hash = hl.sha256(gray.tobytes()).digest()
gray_hash = hl.sha256(gray.tobytes()).digest()
alpha_hash = hl.sha256(alpha.tobytes()).digest()
decomp_hash = hl.sha256(decompressed).digest()
raw_hash = hl.sha256(raw).digest()
addr = bytes.fromhex("ff2142e98e09b5344994f9beb9c56c95506b9f17")
addr_str = b"0xFF2142E98E09b5344994F9bEB9C56C95506B9F17"

for key_material in [img_hash, gray_hash, alpha_hash, decomp_hash, raw_hash,
                     gray.tobytes()[:64], alpha.tobytes()[:64],
                     decompressed[:64], raw[:64]]:
    for message in [addr, addr_str, b"ArweaveP", b"PZL11",
                    img_hash, gray_hash, alpha_hash]:
        # HMAC-SHA256
        mac = hmac.new(key_material, message, hl.sha256).digest()
        check_and_report(mac, f"HMAC-SHA256 key={key_material[:8].hex()} msg={message[:8]}")

        # HMAC-SHA512, take first 32 bytes
        mac = hmac.new(key_material, message, hl.sha512).digest()[:32]
        check_and_report(mac, f"HMAC-SHA512 key={key_material[:8].hex()} msg={message[:8]}")

print(f"  HMAC done. Tests: {total_tests}")

print("\n=== APPROACH: Gray - Alpha, Alpha - Gray ===")
# Try element-wise operations
for op_name, op_fn in [
    ("gray-alpha", lambda g, a: np.subtract(g.astype(np.int16), a.astype(np.int16)) % 256),
    ("alpha-gray", lambda a, g: np.subtract(a.astype(np.int16), g.astype(np.int16)) % 256),
    ("gray+alpha", lambda g, a: np.add(g.astype(np.int16), a.astype(np.int16)) % 256),
    ("gray*alpha", lambda g, a: np.multiply(g.astype(np.int16), a.astype(np.int16)) % 256),
    ("gray^alpha", lambda g, a: np.bitwise_xor(g, a)),
    ("gray&alpha", lambda g, a: np.bitwise_and(g, a)),
    ("gray|alpha", lambda g, a: np.bitwise_or(g, a)),
]:
    result = op_fn(gray, alpha).flatten().astype(np.uint8)
    step = max(1, len(result) // 500000)
    for start in range(0, len(result) - 32, step):
        key = bytes(result[start:start+32])
        check_and_report(key, f"{op_name} offset {start}")

print(f"  Pixel ops done. Tests: {total_tests}")

print("\n=== APPROACH: Iterate through ALL pixel operations on boat ===")
boat_gray = gray[300:700, 0:500].flatten()
boat_alpha = alpha[300:700, 0:500].flatten()

for op_name, op_fn in [
    ("g-a", lambda g, a: np.subtract(g.astype(np.int16), a.astype(np.int16)) % 256),
    ("a-g", lambda a, g: np.subtract(a.astype(np.int16), g.astype(np.int16)) % 256),
    ("g+a", lambda g, a: np.add(g.astype(np.int16), a.astype(np.int16)) % 256),
    ("g^a", lambda g, a: np.bitwise_xor(g, a)),
]:
    result = op_fn(boat_gray, boat_alpha).astype(np.uint8)
    for start in range(0, len(result) - 32, 1):
        key = bytes(result[start:start+32])
        check_and_report(key, f"Boat {op_name} offset {start}")

print(f"  Boat ops done. Tests: {total_tests}")

print("\n=== APPROACH: SHA256 with various iterations of address ===")
for i in range(1, 10000):
    h = hashlib.sha256(addr * i).digest()
    if check_and_report(h, f"SHA256(addr*{i})"):
        break
    h = hashlib.sha256(addr_str * i).digest()
    if check_and_report(h, f"SHA256(addr_str*{i})"):
        break

print(f"  Address hashing done. Tests: {total_tests}")

print(f"\nTotal tests: {total_tests}")
