from PIL import Image
import numpy as np
import struct, zlib
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
filters = []
pr = None
for ri in range(1105):
    s = ri * (1 + 1600*2)
    ft = decompressed[s]
    rd = decompressed[s+1:s+1+1600*2]
    filters.append(ft)
    rows_data.append(undo_filter(ft, rd, pr))
    pr = rows_data[-1]

gray = np.zeros((1105,1600), dtype=np.uint8)
alpha = np.zeros((1105,1600), dtype=np.uint8)
for ri, rd in enumerate(rows_data):
    for c in range(1600):
        gray[ri,c] = rd[c*2]
        alpha[ri,c] = rd[c*2+1]

print("=== Focus: Row-by-row dominant alpha values ===")
# For each row, find the dominant non-zero, non-255 alpha value
# This reveals the encoded data in the alpha channel
dominant_alpha = []
for row in range(1105):
    alpha_row = alpha[row, :]
    unique, counts = np.unique(alpha_row, return_counts=True)
    # Find the most common non-zero, non-255 value
    mask = (unique != 0) & (unique != 255)
    if np.any(mask):
        filtered_unique = unique[mask]
        filtered_counts = counts[mask]
        best_idx = np.argmax(filtered_counts)
        dominant_alpha.append(int(filtered_unique[best_idx]))
    else:
        dominant_alpha.append(-1)  # All 0 or all 255

# Show the dominant alpha pattern
print("Dominant non-trivial alpha per row (non-zero, non-255):")
prev = -2
run_start = 0
runs = []
for i, v in enumerate(dominant_alpha):
    if v != prev:
        if prev >= 0:
            runs.append((run_start, i-1, prev, i - run_start))
        run_start = i
        prev = v
if prev >= 0:
    runs.append((run_start, len(dominant_alpha)-1, prev, len(dominant_alpha) - run_start))

for start, end, val, length in runs:
    if length > 1:
        print(f"  Rows {start}-{end}: alpha={val} (length={length})")

print(f"\n=== Try: Use run-length encoded alpha pattern as key ===")
# The dominant alpha values for each row form a sequence
# Filter out rows with no dominant value (-1)
alpha_sequence = [v for v in dominant_alpha if v >= 0]
print(f"  Alpha sequence length: {len(alpha_sequence)}")
print(f"  First 64 values: {alpha_sequence[:64]}")

# Try as bytes directly
for start in range(0, len(alpha_sequence) - 32, 1):
    key = bytes(alpha_sequence[start:start+32])
    check_and_report(key, f"Alpha dominant sequence offset {start}")

# Try as nibble pairs
if len(alpha_sequence) >= 64:
    for start in range(0, len(alpha_sequence) - 64, 1):
        nibbles = alpha_sequence[start:start+64]
        hex_str = ''.join(f'{n % 16:x}' for n in nibbles)
        try:
            key = bytes.fromhex(hex_str)
            check_and_report(key, f"Alpha dominant nibbles offset {start}")
        except:
            pass

print(f"\n=== Try: Row-by-row grayscale hash ===")
for row in range(300, 700):
    row_bytes = gray[row, :].tobytes()
    # Try hash of each row
    key = hashlib.sha256(row_bytes).digest()
    check_and_report(key, f"SHA256 of row {row}")

print(f"\n=== Try: Hash of pixel pairs ===")
# Hash of consecutive rows combined
for row in range(300, 699):
    combined = gray[row, :].tobytes() + gray[row+1, :].tobytes()
    key = hashlib.sha256(combined).digest()
    check_and_report(key, f"SHA256 of rows {row}-{row+1}")

print(f"\n=== Try: Every row's dominant gray value as key ===")
row_dominant_gray = []
for row in range(1105):
    row_gray = gray[row, :]
    unique, counts = np.unique(row_gray, return_counts=True)
    # Most common value
    best_idx = np.argmax(counts)
    row_dominant_gray.append(int(unique[best_idx]))

print(f"  First 64: {row_dominant_gray[:64]}")
for start in range(0, len(row_dominant_gray) - 32, 1):
    key = bytes(row_dominant_gray[start:start+32])
    check_and_report(key, f"Dominant gray per row offset {start}")

print(f"\n=== Try: Hash of the entire sailboat with alpha mask ===")
# Extract ONLY non-transparent pixels in the sailboat region
boat_y1, boat_y2, boat_x1, boat_x2 = 300, 700, 0, 500
boat_masked = []
for y in range(boat_y1, boat_y2):
    for x in range(boat_x1, boat_x2):
        if alpha[y, x] > 0:
            boat_masked.append(int(gray[y, x]))
print(f"  Sailboat non-transparent pixels: {len(boat_masked)}")
boat_bytes = bytes(boat_masked)
key = hashlib.sha256(boat_bytes).digest()
check_and_report(key, "SHA256 of sailboat non-transparent pixels")
key = hashlib.sha512(boat_bytes).digest()[:32]
check_and_report(key, "SHA512 of sailboat non-transparent pixels")

# Try double hash
key = hashlib.sha256(hashlib.sha256(boat_bytes).digest()).digest()
check_and_report(key, "Double SHA256 of sailboat non-transparent pixels")

print(f"\n=== Try: Key from raw PNG file bytes ===")
# Maybe the key is hidden somewhere in the raw PNG bytes
for offset in range(0, len(raw) - 32):
    key = raw[offset:offset+32]
    check_and_report(key, f"Raw PNG bytes offset {offset}")

# Hash of the entire PNG file
key = hashlib.sha256(raw).digest()
check_and_report(key, "SHA256 of entire PNG file")
key = hashlib.sha3_256(raw).digest()
check_and_report(key, "SHA3-256 of entire PNG file")
key = hashlib.md5(raw).digest()
check_and_report(key, "MD5 of entire PNG file")

# Hash of PNG without the8-byte signature
key = hashlib.sha256(raw[8:]).digest()
check_and_report(key, "SHA256 of PNG without signature")

# Hash of just IDAT data
key = hashlib.sha256(idat_raw).digest()
check_and_report(key, "SHA256 of raw IDAT data")
key = hashlib.sha256(decompressed).digest()
check_and_report(key, "SHA256 of decompressed IDAT")

# Hash combinations
all_chunks = b''
pos = 8
while pos < len(raw):
    length = struct.unpack('>I', raw[pos:pos+4])[0]
    ct = raw[pos+4:pos+8]
    cd = raw[pos+8:pos+8+length]
    if ct not in [b'IDAT', b'IEND']:
        all_chunks += cd
    pos += 12 + length
key = hashlib.sha256(all_chunks).digest()
check_and_report(key, "SHA256 of all non-IDAT chunks")
key = hashlib.sha256(all_chunks + decompressed).digest()
check_and_report(key, "SHA256 of non-IDAT chunks + decompressed")

# SHA256 of specific combinations
key = hashlib.sha256(decompressed + idat_raw).digest()
check_and_report(key, "SHA256 of decompressed + raw IDAT")
key = hashlib.sha256(idat_raw + decompressed).digest()
check_and_report(key, "SHA256 of raw IDAT + decompressed")

print(f"\n=== Try: SHA256 of pixel data with address as salt ===")
address = bytes.fromhex("ff2142e98e09b5344994f9beb9c56c95506b9f17")
key = hashlib.sha256(gray.tobytes() + address).digest()
check_and_report(key, "SHA256(gray + address)")
key = hashlib.sha256(address + gray.tobytes()).digest()
check_and_report(key, "SHA256(address + gray)")
key = hashlib.sha256(alpha.tobytes() + address).digest()
check_and_report(key, "SHA256(alpha + address)")
key = hashlib.sha256(address + alpha.tobytes()).digest()
check_and_report(key, "SHA256(address + alpha)")
key = hashlib.sha256(decompressed + address).digest()
check_and_report(key, "SHA256(decompressed + address)")
key = hashlib.sha256(address + decompressed).digest()
check_and_report(key, "SHA256(address + decompressed)")

# Date strings
date1 = b"2020-03-30T11:38:07+03:00"
date2 = b"2020-03-30T11:34:44+03:00"
key = hashlib.sha256(decompressed + date1).digest()
check_and_report(key, "SHA256(decompressed + date1)")
key = hashlib.sha256(decompressed + date2).digest()
check_and_report(key, "SHA256(decompressed + date2)")
key = hashlib.sha256(decompressed + date1 + date2).digest()
check_and_report(key, "SHA256(decompressed + dates)")
key = hashlib.sha256(date1 + date2 + decompressed).digest()
check_and_report(key, "SHA256(dates + decompressed)")

print(f"\n=== Try: PBKDF2 with image-derived salt ===")
for salt in [gray[:32, :32].tobytes(), alpha[:32, :32].tobytes(),
             hashlib.sha256(gray.tobytes()).digest(),
             hashlib.sha256(decompressed).digest()]:
    for passphrase in [b"ArweaveP", b"arweavepuzzle", b"PZL11", b"puzzle11",
                       address, b"0xFF2142E98E09b5344994F9bEB9C56C95506B9F17",
                       b"comment", b"twitter.com/ArweaveP",
                       b"private key", b"secret", b"hidden key",
                       b"PK is on the biggest boat"]:
        key = hashlib.pbkdf2_hmac('sha256', passphrase, salt, 1, dklen=32)
        check_and_report(key, f"PBKDF2(1 iter) pass={passphrase[:20]} salt={salt[:8].hex()}")
        key = hashlib.pbkdf2_hmac('sha256', passphrase, salt, 100, dklen=32)
        check_and_report(key, f"PBKDF2(100 iter) pass={passphrase[:20]}")
        key = hashlib.pbkdf2_hmac('sha256', passphrase, salt, 1000, dklen=32)
        check_and_report(key, f"PBKDF2(1000) pass={passphrase[:20]}")

print(f"\n=== Try: scrypt ===")
try:
    for passphrase in [b"ArweaveP", address, b"PZL11", b"puzzle11", b"comment"]:
        for salt in [b"salt", hashlib.sha256(gray.tobytes()).digest()[:16]]:
            key = hashlib.scrypt(passphrase, salt=salt, n=1024, r=8, p=1, dklen=32)
            check_and_report(key, f"scrypt pass={passphrase[:16]} salt={salt[:8]}")
except:
    print("  scrypt failed")

print(f"\nTotal tests: {total_tests}")
