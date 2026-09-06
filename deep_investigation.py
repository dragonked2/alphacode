from PIL import Image
import numpy as np
import struct, zlib
from ecdsa import SigningKey, SECP256k1
import hashlib
from collections import Counter

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
    return False

total_tests = 0

with open('puzzle_image.png', 'rb') as f:
    raw = f.read()

# Parse all chunks
pos = 8
chunks = []
idat_raw = b''
while pos < len(raw):
    length = struct.unpack('>I', raw[pos:pos+4])[0]
    ct = raw[pos+4:pos+8]
    cd = raw[pos+8:pos+8+length]
    crc = raw[pos+8+length:pos+12+length]
    chunks.append((ct, cd, length))
    if ct == b'IDAT':
        idat_raw += cd
    pos += 12 + length

print("=== All PNG chunks ===")
for ct, cd, l in chunks:
    print(f"  {ct}: {l} bytes")
    if ct not in [b'IDAT', b'IEND']:
        print(f"    Hex: {cd.hex()}")
        print(f"    Raw: {cd}")

# Check for trailing data after IEND
iend_pos = raw.find(b'IEND')
if iend_pos >= 0:
    after_iend = raw[iend_pos+8:]  # 4 bytes chunk type + 4 bytes CRC after IEND
    print(f"\n  Data after IEND chunk: {len(after_iend)} bytes")
    if len(after_iend) > 0:
        print(f"  Hex: {after_iend.hex()[:200]}")
        # Try these bytes as key
        if len(after_iend) >= 32:
            check_and_report(after_iend[:32], "Data after IEND")

decompressed = zlib.decompress(idat_raw)
print(f"\n  Compressed IDAT: {len(idat_raw)} bytes")
print(f"  Decompressed: {len(decompressed)} bytes")
print(f"  Expected (1600*1105*2+1105): {1600*1105*2+1105}")

# Check if there's extra data after valid zlib stream
try:
    result = zlib.decompress(idat_raw)
    # Try to decompress again with wbits=-15 (raw deflate)
    # Check if there are bytes after the zlib stream
    z = zlib.decompressobj()
    decompressed2 = z.decompress(idat_raw)
    remaining = z.flush()
    unconsumed = idat_raw[len(idat_raw) - len(z.unused_data):] if hasattr(z, 'unused_data') else b''
    print(f"  Zlib unused_data: {len(z.unused_data)} bytes")
    if len(z.unused_data) > 0:
        print(f"  Unused data hex: {z.unused_data.hex()}")
        if len(z.unused_data) >= 32:
            check_and_report(z.unused_data[:32], "Zlib unused data")
except Exception as e:
    print(f"  Error: {e}")

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
filters = []
pr = None
for ri in range(1105):
    s = ri * (1 + 1600*2)
    ft = decompressed[s]
    rd = decompressed[s+1:s+1+1600*2]
    filters.append(ft)
    rows.append(undo_filter(ft, rd, pr))
    pr = rows[-1]

gray = np.zeros((1105,1600), dtype=np.uint8)
alpha = np.zeros((1105,1600), dtype=np.uint8)
for ri, rd in enumerate(rows):
    for c in range(1600):
        gray[ri,c] = rd[c*2]
        alpha[ri,c] = rd[c*2+1]

print(f"\n=== Filter type distribution ===")
filter_counts = Counter(filters)
print(f"  {dict(filter_counts)}")

# The filter bytes themselves could encode data
filter_bytes = bytes(filters)
print(f"  First 32 filter bytes: {filter_bytes[:32]}")
print(f"  All filter bytes: {filter_bytes.hex()}")
if len(filter_bytes) >= 32:
    check_and_report(filter_bytes[:32], "First 32 filter bytes as key")

print(f"\n=== Grayscale value distribution (unique values) ===")
gray_flat = gray.flatten()
unique_gray = sorted(set(gray_flat))
print(f"  Unique values: {len(unique_gray)}")
print(f"  Most common: {Counter(gray_flat).most_common(10)}")
print(f"  Least common: {Counter(gray_flat).most_common()[-10:]}")

print(f"\n=== Non-255, non-0 grayscale values ===")
non_std = [(i, int(v)) for i, v in enumerate(gray_flat) if v not in [0, 255]]
print(f"  Count: {len(non_std)}")

# Non-standard grayscale values as bytes
non_std_vals = [v for _, v in non_std]
if len(non_std_vals) >= 32:
    # Try first 32
    check_and_report(bytes(non_std_vals[:32]), "First 32 non-std gray values")
    # Try every 16th
    sampled = non_std_vals[::16]
    if len(sampled) >= 32:
        check_and_report(bytes(sampled[:32]), "Non-std gray sampled every 16th")
    # Try sorted by position, take every Nth
    for stride in [2, 3, 4, 5, 7, 8, 11, 13]:
        sampled = non_std_vals[::stride]
        if len(sampled) >= 32:
            check_and_report(bytes(sampled[:32]), f"Non-std gray stride {stride}")

print(f"\n=== Alpha values at non-255 positions ===")
alpha_flat = alpha.flatten()
non_255_alpha = [(i, int(alpha_flat[i])) for i in range(len(alpha_flat)) if alpha_flat[i] != 255]
print(f"  Count: {len(non_255_alpha)}")

# GRAYSCALE values AT the non-255 alpha positions
gray_at_aa = [int(gray_flat[i]) for i, _ in non_255_alpha]
print(f"  Gray values at AA positions: {gray_at_aa[:32]}")
print(f"  Count: {len(gray_at_aa)}")
if len(gray_at_aa) >= 32:
    check_and_report(bytes(gray_at_aa[:32]), "Gray values at alpha-AA positions")
    check_and_report(bytes(gray_at_aa[-32:]), "Last 32 gray values at alpha-AA positions")
    # Try reversed
    check_and_report(bytes(gray_at_aa[:32][::-1]), "Reversed gray at AA positions")
    # Try with alpha values XORed with gray values
    xor_vals = [gray_at_aa[i] ^ non_255_alpha[i][1] for i in range(min(32, len(gray_at_aa)))]
    check_and_report(bytes(xor_vals), "Gray XOR alpha at AA positions")

print(f"\n=== Try: IDAT decompressed data as key source ===")
# The decompressed data is 3,536,105 bytes
# Try reading from specific offsets
for offset in range(0, min(10000, len(decompressed) - 32), 1):
    check_and_report(decompressed[offset:offset+32], f"Decompressed byte offset {offset}")

# Try reading from the END
for offset in range(max(0, len(decompressed) - 10000), len(decompressed) - 32, 1):
    check_and_report(decompressed[offset:offset+32], f"Decompressed byte offset from end {offset}")

# Try raw IDAT bytes
for offset in range(0, min(len(idat_raw) - 32, 10000), 1):
    check_and_report(idat_raw[offset:offset+32], f"Raw IDAT offset {offset}")

print(f"\n=== Try: Specific row values from the image ===")
# The author said "PK is on the biggest boat"
# Let's look at EVERY row that intersects the boat and try different interpretations
for row in range(300, 750):
    row_data = gray[row, :500]  # Boat region columns
    # Try: all non-zero values
    non_zero = [int(v) for v in row_data if v > 0]
    if len(non_zero) == 32:
        check_and_report(bytes(non_zero), f"Row {row} non-zero values")
    if len(non_zero) == 64:
        # Try as hex: pair up values as nibbles
        for i in range(0, 64, 2):
            nibbles = non_zero[i:i+32]
            hex_str = ''.join(f'{n:x}' for n in nibbles)
            if len(hex_str) == 64:
                try:
                    check_and_report(bytes.fromhex(hex_str), f"Row {row} nibble pair offset {i}")
                except:
                    pass

print(f"\nTotal tests: {total_tests}")
