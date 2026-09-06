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

# Parse ALL chunks properly
pos = 8
chunks = []
idat_raw = b''
while pos < len(raw):
    length = struct.unpack('>I', raw[pos:pos+4])[0]
    ct = raw[pos+4:pos+8]
    cd = raw[pos+8:pos+8+length]
    crc = struct.unpack('>I', raw[pos+8+length:pos+12+length])[0]
    chunks.append((ct, cd, length, crc))
    if ct == b'IDAT':
        idat_raw += cd
    pos += 12 + length

print("=== Complete chunk analysis ===")
for ct, cd, l, crc in chunks:
    name = ct.decode('ascii', errors='replace')
    print(f"\n--- {name} ({l} bytes) ---")
    if l <= 64:
        print(f"  Hex: {cd.hex()}")
    if name == 'cHRM':
        # Parse the 8 values (each 4 bytes big-endian unsigned)
        vals = struct.unpack('>8I', cd)
        labels = ['wx', 'wy', 'rx', 'ry', 'gx', 'gy', 'bx', 'by']
        for lbl, val in zip(labels, vals):
            actual = val / 100000.0
            print(f"  {lbl} = {val} ({actual})")
        print(f"  Standard sRGB: wx=31270, wy=32900, rx=64000, ry=33000, gx=30000, gy=60000, bx=15000, by=6000")
    elif name == 'gAMA':
        val = struct.unpack('>I', cd)[0]
        print(f"  Gamma: {val} ({val/100000.0})")
    elif name == 'pHYs':
        px, py, unit = struct.unpack('>IIB', cd)
        print(f"  px={px} py={py} unit={unit}")
    elif name == 'IHDR':
        w, h, bd, ct, cm, fm, il = struct.unpack('>IIBBBBB', cd)
        print(f"  {w}x{h}, bit_depth={bd}, color_type={ct}, compression={cm}, filter={fm}, interlace={il}")
    elif name == 'bKGD':
        print(f"  Raw: {cd.hex()}")

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

print(f"\n=== Alpha channel detailed analysis ===")
alpha_flat = alpha.flatten()
alpha_counts = Counter(alpha_flat)
print(f"Unique alpha values: {len(alpha_counts)}")
print(f"Top 20: {alpha_counts.most_common(20)}")
print(f"Bottom 20: {alpha_counts.most_common()[-20:]}")

# Show ALL alpha values that are NOT 0 and NOT 255
non_standard_alpha = {k: v for k, v in alpha_counts.items() if k not in [0, 255]}
print(f"\nNon-standard alpha values: {len(non_standard_alpha)}")
for val in sorted(non_standard_alpha.keys()):
    print(f"  alpha={val}: {non_standard_alpha[val]} pixels")

# Find the positions of pixels with non-standard alpha
print(f"\n=== Locating non-standard alpha pixels ===")
for target_alpha in sorted(non_standard_alpha.keys()):
    positions = np.where(alpha == target_alpha)
    if len(positions[0]) > 0:
        y_min, y_max = positions[0].min(), positions[0].max()
        x_min, x_max = positions[1].min(), positions[1].max()
        print(f"  alpha={target_alpha}: {len(positions[0])} pixels, "
              f"y:[{y_min}-{y_max}], x:[{x_min}-{x_max}]")

# Now the key question: could these alpha pixels encode the key?
# The earlier investigation said there were only 434 non-255 values
# But we now see 198924 non-zero, non-255 alpha values
# Let me look at the alpha values that are in the range 1-254 but not 0
print(f"\n=== Alpha value 1-254 detailed position map ===")
for target_alpha in [1, 2, 128, 229, 230, 254]:
    positions = np.where(alpha == target_alpha)
    if len(positions[0]) > 0:
        print(f"  alpha={target_alpha}: {len(positions[0])} pixels, "
              f"y:[{positions[0].min()}-{positions[0].max()}], "
              f"x:[{positions[1].min()}-{positions[1].max()}]")

print(f"\n=== Try: Read alpha values in specific order for key extraction ===")
# Get all unique alpha values (non-zero, non-255) in raster order
non_std_alpha_pixels = []
for y in range(1105):
    for x in range(1600):
        a = int(alpha[y, x])
        if a != 0 and a != 255:
            non_std_alpha_pixels.append((y, x, a))

print(f"  Non-zero, non-255 alpha pixels: {len(non_std_alpha_pixels)}")

# Group by alpha value
for target in sorted(set(v for _, _, v in non_std_alpha_pixels)):
    pixels = [(y, x) for y, x, v in non_std_alpha_pixels if v == target]
    print(f"  alpha={target}: {len(pixels)} pixels")

# What if specific alpha values represent hex digits?
# alpha=1 could be 0x1, alpha=2 could be 0x2...
# Try reading ALL non-zero alpha values as hex nibbles
alpha_at_nonzero = [v for _, _, v in non_std_alpha_pixels]
print(f"\n  First 64 alpha values (non-zero non-255): {alpha_at_nonzero[:64]}")

# Try pairing nibbles
if len(alpha_at_nonzero) >= 64:
    hex_str = ''.join(f'{v:x}' for v in alpha_at_nonzero[:64])
    # Only use if all characters are valid hex
    try:
        key = bytes.fromhex(hex_str[:64])
        check_and_report(key, "Alpha nibbles as hex")
    except:
        pass

# What about using the GRAYSCALE values at positions where alpha is in a specific range?
# Focus on the sailboat: y:300-700, x:0-500
print(f"\n=== Focus: Unique grayscale patterns on the sailboat ===")
boat_gray = gray[300:700, 0:500]
boat_alpha = alpha[300:700, 0:500]

# Find pixels that have dark gray (the drawing) and non-255 alpha
boat_special = []
for y in range(boat_gray.shape[0]):
    for x in range(boat_gray.shape[1]):
        g = int(boat_gray[y, x])
        a = int(boat_alpha[y, x])
        if g > 0 and g < 255:
            boat_special.append((y + 300, x, g, a))

print(f"  Non-0, non-255 gray pixels in boat: {len(boat_special)}")

# Now try: what if the KEY is encoded as specific grayscale values on the boat?
# The author said PK is on the biggest boat
# Let's try reading the boat in different orders

# Read the boat in raster order (already tested)
# Read only the dark pixels (drawing lines)
dark_boat = [(y, x, g, a) for y, x, g, a in boat_special if g < 128]
print(f"  Dark pixels (gray < 128) on boat: {len(dark_boat)}")

if len(dark_boat) >= 32:
    # Try first 32 dark pixel gray values
    key = bytes([g for _, _, g, _ in dark_boat[:32]])
    check_and_report(key, "First 32 dark pixel gray values on boat")
    
    # Try with specific stride
    for stride in [1, 2, 3, 4, 5, 7, 11, 13, 17, 19, 23, 29, 31]:
        sampled = dark_boat[::stride]
        if len(sampled) >= 32:
            key = bytes([g for _, _, g, _ in sampled[:32]])
            check_and_report(key, f"Dark boat pixels stride {stride}")

print(f"\n=== Try: cHRM bytes interpreted differently ===")
# The cHRM chunk has 32 bytes. What if the bytes encode the key directly?
# But we already checked: they're standard sRGB values
# Let me try treating them as raw key material anyway
cHRM_data = None
for ct, cd, l, crc in chunks:
    if ct == b'cHRM':
        cHRM_data = cd
        break

if cHRM_data:
    # Try as key directly
    check_and_report(cHRM_data[:32], "cHRM raw bytes as key")
    
    # Try XORing the values
    vals = struct.unpack('>8I', cHRM_data)
    # XOR all values together
    xor_result = 0
    for v in vals:
        xor_result ^= v
    key = xor_result.to_bytes(4, 'big') * 8
    check_and_report(key, "cHRM values XORed")
    
    # Try treating as nibbles
    nibbles = []
    for v in vals:
        nibbles.extend([(v >> 28) & 0xF, (v >> 24) & 0xF, (v >> 20) & 0xF, (v >> 16) & 0xF,
                       (v >> 12) & 0xF, (v >> 8) & 0xF, (v >> 4) & 0xF, v & 0xF])
    if len(nibbles) >= 64:
        hex_str = ''.join(f'{n:x}' for n in nibbles[:64])
        try:
            key = bytes.fromhex(hex_str[:64])
            check_and_report(key, "cHRM nibbles as hex key")
        except:
            pass

print(f"\n=== Try: pHYs, gAMA, bKGD as key material ===")
all_metadata = b''
for ct, cd, l, crc in chunks:
    if ct not in [b'IDAT', b'IEND']:
        all_metadata += cd

print(f"  All metadata bytes: {len(all_metadata)}")
print(f"  Hex: {all_metadata.hex()}")
check_and_report(all_metadata[:32], "First 32 metadata bytes")
if len(all_metadata) >= 64:
    check_and_report(all_metadata[:64], "First 64 metadata bytes")

# Hash of all metadata
key = hashlib.sha256(all_metadata).digest()
check_and_report(key, "SHA256 of all metadata")
key = hashlib.sha3_256(all_metadata).digest()
check_and_report(key, "SHA3-256 of all metadata")

# Date strings as key material
date_create = b'2020-03-30T11:38:07+03:00'
date_modify = b'2020-03-30T11:34:44+03:00'
key = hashlib.sha256(date_create + date_modify).digest()
check_and_report(key, "SHA256 of dates")
key = hashlib.sha256(date_modify + date_create).digest()
check_and_report(key, "SHA256 of dates reversed")

# Address as key material
address = b'0xFF2142E98E09b5344994F9bEB9C56C95506B9F17'
key = hashlib.sha256(address).digest()
check_and_report(key, "SHA256 of address string")

print(f"\n=== Try: Filter types as binary data ===")
filter_hex = ''.join(str(f) for f in filters)
# Try as binary: 0->0, 1->00, 2->01, 3->10, 4->11
binary_str = ''
for f in filters:
    if f == 0: binary_str += '00'
    elif f == 1: binary_str += '01'
    elif f == 2: binary_str += '10'
    elif f == 3: binary_str += '11'
    elif f == 4: binary_str += '100'

print(f"  Filter binary string length: {len(binary_str)}")
# Take first 256 bits as key
for start in range(0, len(binary_str) - 256, 1):
    key_bytes = bytearray()
    for i in range(32):
        byte_val = 0
        for j in range(8):
            byte_val = (byte_val << 1) | int(binary_str[start + i*8 + j])
        key_bytes.append(byte_val)
    check_and_report(bytes(key_bytes), f"Filter binary at offset {start}")

# Alternative: filter types as octal/trinary
print(f"\n  Filter types: {filters[:50]}...")

print(f"\nTotal tests: {total_tests}")
