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

total = 0

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

gray_flat = gray.flatten()
total_pixels = len(gray_flat)
print(f"Total pixels: {total_pixels}")
print(f"Factors: trying reshape approach")

print("\n=== RESHAPE APPROACH ===")
print("Try reshaping the 1D pixel array to various dimensions")
print("and check if any row/column contains 32 consecutive bytes of the key")

# The image has 1,768,000 pixels
# For the key to appear as 32 consecutive pixels in a reshaped image,
# the reshape width must be such that 32 pixels fit in one row

# Try all widths that divide 1768000 or close to it
factors = []
for w in range(1, min(2000, total_pixels)):
    if total_pixels % w == 0:
        factors.append(w)
        
print(f"Exact factors up to 2000: {len(factors)}")
print(f"Sample factors: {factors[:20]}...{factors[-20:]}")

# For each factor width, reshape and check rows for 32-byte key
for w in factors:
    h = total_pixels // w
    if w < 32:
        continue  # Need at least 32 pixels wide for the key
    
    reshaped = gray_flat.reshape(h, w)
    
    # Check each row for 32 consecutive pixels that form a valid key
    for row_idx in range(h):
        row = reshaped[row_idx]
        for start in range(0, w - 32):
            key = bytes(row[start:start+32])
            total += 1
            if check_key(key):
                print(f"\n*** SOLVED! Width={w}, Row={row_idx}, Start={start}")
                print(f"Key: {key.hex()}")
                exit(0)
    
    # Also check each column for 32 consecutive pixels
    for col_idx in range(w):
        col = reshaped[:, col_idx]
        for start in range(0, h - 32):
            key = bytes(col[start:start+32])
            total += 1
            if check_key(key):
                print(f"\n*** SOLVED! Width={w}, Col={col_idx}, Start={start}")
                print(f"Key: {key.hex()}")
                exit(0)
    
    if total % 100000 == 0:
        print(f"  Tests so far: {total} (width={w})")

print(f"\nFactor-based reshape done. Tests: {total}")

# Also try common non-factor widths with zero-padding
print("\n=== RESHAPE WITH PADDING ===")
for w in [64, 128, 256, 320, 512, 640]:
    h_needed = (total_pixels + w - 1) // w
    padded = np.zeros(h_needed * w, dtype=np.uint8)
    padded[:total_pixels] = gray_flat
    reshaped = padded.reshape(h_needed, w)
    
    for row_idx in range(min(h_needed, 10)):
        for start in range(0, w - 32):
            key = bytes(reshaped[row_idx, start:start+32])
            total += 1
            if check_key(key):
                print(f"\n*** SOLVED! Padded width={w}, Row={row_idx}, Start={start}")
                print(f"Key: {key.hex()}")
                exit(0)
    
    for col_idx in range(min(w, 64)):
        col = reshaped[:, col_idx]
        for start in range(0, h_needed - 32):
            key = bytes(col[start:start+32])
            total += 1
            if check_key(key):
                print(f"\n*** SOLVED! Padded width={w}, Col={col_idx}, Start={start}")
                print(f"Key: {key.hex()}")
                exit(0)

print(f"Padded reshape done. Tests: {total}")

# Try column-major reading of original image
print("\n=== COLUMN-MAJOR READING ===")
gray_col_major = gray.T.flatten()
for start in range(0, total_pixels - 32, max(1, total_pixels // 1000000)):
    key = bytes(gray_col_major[start:start+32])
    total += 1
    if check_key(key):
        print(f"\n*** SOLVED! Column-major offset {start}")
        print(f"Key: {key.hex()}")
        exit(0)

print(f"Column-major done. Tests: {total}")

# Try diagonal reading
print("\n=== DIAGONAL READING ===")
for start_y in range(min(1105, 100)):
    for start_x in range(min(1600, 100)):
        vals = []
        y, x = start_y, start_x
        while len(vals) < 32 and y < 1105 and x < 1600:
            vals.append(int(gray[y, x]))
            y += 1
            x += 1
        if len(vals) == 32:
            key = bytes(vals)
            total += 1
            if check_key(key):
                print(f"\n*** SOLVED! Diagonal from ({start_y},{start_x})")
                print(f"Key: {key.hex()}")
                exit(0)

# Try anti-diagonal
for start_y in range(min(1105, 100)):
    for start_x in range(max(0, 1600-100), 1600):
        vals = []
        y, x = start_y, start_x
        while len(vals) < 32 and y < 1105 and x >= 0:
            vals.append(int(gray[y, x]))
            y += 1
            x -= 1
        if len(vals) == 32:
            key = bytes(vals)
            total += 1
            if check_key(key):
                print(f"\n*** SOLVED! Anti-diagonal from ({start_y},{start_x})")
                print(f"Key: {key.hex()}")
                exit(0)

print(f"Diagonal done. Tests: {total}")

# Try: what if the key bytes are the differences between consecutive non-255 pixels?
print("\n=== PIXEL DIFFERENCES ===")
non_255 = [int(v) for v in gray_flat if v != 255]
print(f"Non-255 pixels: {len(non_255)}")
if len(non_255) >= 64:
    # Try differences
    diffs = [non_255[i+1] - non_255[i] for i in range(len(non_255)-1)]
    for start in range(0, len(diffs) - 32):
        key = bytes([(d % 256) for d in diffs[start:start+32]])
        total += 1
        if check_key(key):
            print(f"\n*** SOLVED! Pixel differences at {start}")
            print(f"Key: {key.hex()}")
            exit(0)

print(f"Pixel differences done. Tests: {total}")

# Try: read only the DARK pixels (the actual sketch lines)
print("\n=== DARK PIXELS ONLY (gray < 128) ===")
dark_pixels = [int(v) for v in gray_flat if v < 128]
print(f"Dark pixels: {len(dark_pixels)}")
for start in range(0, max(0, len(dark_pixels) - 32), max(1, len(dark_pixels)//500000)):
    key = bytes(dark_pixels[start:start+32])
    total += 1
    if check_key(key):
        print(f"\n*** SOLVED! Dark pixels at {start}")
        print(f"Key: {key.hex()}")
        exit(0)

print(f"Dark pixels done. Tests: {total}")

# Try: read the GRAYSCALE values of only the boat hull region,
# but using specific patterns
print("\n=== BOAT HULL - SPECIFIC PATTERNS ===")
# Hull is roughly y:550-700, x:44-360
hull = gray[550:700, 44:360]
hull_flat = hull.flatten()

# Try reading in zigzag pattern
zigzag = []
for diag in range(hull.shape[0] + hull.shape[1]):
    if diag % 2 == 0:
        for y in range(max(0, diag - hull.shape[1] + 1), min(diag + 1, hull.shape[0])):
            x = diag - y
            if 0 <= x < hull.shape[1]:
                zigzag.append(int(hull[y, x]))
    else:
        for y in range(min(diag + 1, hull.shape[0]) - 1, max(0, diag - hull.shape[1]) - 1, -1):
            x = diag - y
            if 0 <= x < hull.shape[1]:
                zigzag.append(int(hull[y, x]))

for start in range(0, max(0, len(zigzag) - 32), max(1, len(zigzag)//200000)):
    key = bytes(zigzag[start:start+32])
    total += 1
    if check_key(key):
        print(f"\n*** SOLVED! Hull zigzag at {start}")
        print(f"Key: {key.hex()}")
        exit(0)

print(f"Hull zigzag done. Tests: {total}")

print(f"\nFINAL TOTAL: {total}")
