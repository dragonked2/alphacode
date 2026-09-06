from PIL import Image, ImageEnhance, ImageOps, ImageFilter
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

# Load properly decoded pixels
with open('puzzle_image.png', 'rb') as f:
    raw_data = f.read()
pos = 8
idat_data = b''
while pos < len(raw_data):
    length = struct.unpack('>I', raw_data[pos:pos+4])[0]
    chunk_type = raw_data[pos+4:pos+8]
    chunk_data = raw_data[pos+8:pos+8+length]
    if chunk_type == b'IDAT': idat_data += chunk_data
    pos += 12 + length
decompressed = zlib.decompress(idat_data)

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

print("=== Attempt: Very low contrast text extraction ===")
# The key might be written in pixels with very subtle differences
# e.g., 254 vs 253 or 0 vs 1
# Let's try to amplify these tiny differences

# Compute the mean of each row
row_means = np.mean(gray, axis=1)

# For each row, compute deviation from mean
for row_idx in range(1105):
    row = gray[row_idx, :].astype(float)
    deviation = row - row_means[row_idx]
    
    # If there are pixels that deviate significantly from the mean in a pattern
    # that could spell out text...
    # Try to read the deviation as bits
    bits = (deviation > 0).astype(int)
    
    # Convert to bytes
    for start_col in range(0, 1600 - 256, 8):
        key_bytes = bytearray()
        for i in range(32):
            byte_val = 0
            for j in range(8):
                byte_val = (byte_val << 1) | int(bits[start_col + i*8 + j])
            key_bytes.append(byte_val)
        if check_key(bytes(key_bytes)):
            print(f"  MATCH from row {row_idx} deviations, col {start_col}!")

print("=== Attempt: Value differences between adjacent pixels ===")
# Maybe the key is encoded in whether adjacent pixels increase or decrease
for row_idx in range(1105):
    row = gray[row_idx, :].astype(int)
    diffs = np.diff(row)
    bits = (diffs > 0).astype(int)
    
    for start_col in range(0, len(bits) - 256, 8):
        key_bytes = bytearray()
        for i in range(32):
            byte_val = 0
            for j in range(8):
                byte_val = (byte_val << 1) | int(bits[start_col + i*8 + j])
            key_bytes.append(byte_val)
        if check_key(bytes(key_bytes)):
            print(f"  MATCH from row {row_idx} diffs, col {start_col}!")

print("=== Attempt: Vertical differences ===")
for col_idx in range(1600):
    col = gray[:, col_idx].astype(int)
    diffs = np.diff(col)
    bits = (diffs > 0).astype(int)
    
    for start_row in range(0, len(bits) - 256, 8):
        key_bytes = bytearray()
        for i in range(32):
            byte_val = 0
            for j in range(8):
                byte_val = (byte_val << 1) | int(bits[start_row + i*8 + j])
            key_bytes.append(byte_val)
        if check_key(bytes(key_bytes)):
            print(f"  MATCH from col {col_idx} vertical diffs, row {start_row}!")

print("=== Attempt: Even/odd pixel values ===")
for row_idx in range(1105):
    row = gray[row_idx, :]
    bits = row % 2
    
    for start_col in range(0, 1600 - 256, 8):
        key_bytes = bytearray()
        for i in range(32):
            byte_val = 0
            for j in range(8):
                byte_val = (byte_val << 1) | int(bits[start_col + i*8 + j])
            key_bytes.append(byte_val)
        if check_key(bytes(key_bytes)):
            print(f"  MATCH from row {row_idx} even/odd, col {start_col}!")

print("=== Attempt: ASCII values of grayscale pixels in boat region ===")
# The author said PK is on the biggest boat
# Try reading ALL pixels in the boat region as ASCII text
boat_region = gray[320:600, 44:360]
boat_flat = boat_region.flatten()
# Look for 64-char hex strings
for start in range(0, len(boat_flat) - 64):
    window = boat_flat[start:start+64]
    # Check if all values look like hex chars
    valid_hex = True
    for v in window:
        c = int(v)
        if not ((48 <= c <= 57) or (65 <= c <= 70) or (97 <= c <= 102)):
            valid_hex = False
            break
    if valid_hex:
        hex_str = ''.join(chr(int(v)) for v in window)
        try:
            key = bytes.fromhex(hex_str)
            if check_key(key):
                print(f"  MATCH from boat region ASCII at position {start}!")
        except:
            pass

print("=== Attempt: Combining building dimensions with pixel data ===")
building_heights = [100, 190, 90, 165, 240, 55, 180, 160, 90, 160, 100, 190]
building_widths = [115, 90, 100, 170, 100, 100, 150, 110, 110, 100, 70, 50]

# Use building dimensions as seeds to extract specific pixels
for order in ['height', 'width', 'both']:
    if order == 'height':
        dims = building_heights
    elif order == 'width':
        dims = building_widths
    else:
        dims = building_heights + building_widths
    
    for scale in range(1, 21):
        pixels = []
        for d in dims:
            d_scaled = d // scale
            # Use the scaled dimension as a pixel coordinate
            y = d_scaled % 1105
            x = (d_scaled * 7) % 1600  # offset to avoid overlap
            pixels.append(int(gray[y, x]))
        
        key = bytes(pixels[:32]) if len(pixels) >= 32 else b''
        if len(key) == 32 and check_key(key):
            print(f"  MATCH from building {order} scale {scale}!")

print("\nAll attempts exhausted.")
print("This puzzle remains UNSOLVED. It has been open since April 2020.")
print("The exact encoding method used by the author is unknown.")
