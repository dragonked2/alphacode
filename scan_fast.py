import struct, zlib
import numpy as np
import time
from coincurve import PrivateKey
import hashlib

TARGET = bytes.fromhex("ff2142e98e09b5344994f9beb9c56c95506b9f17")

def check_key(key_bytes):
    key_int = int.from_bytes(key_bytes, 'big')
    if key_int == 0 or key_int >= 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141: return False
    try:
        sk = PrivateKey(key_bytes)
        pub = sk.public_key.format(compressed=False)
        addr = hashlib.new('sha3_256', pub).digest()[12:]
        return addr == TARGET
    except: return False

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

# Fast numpy-based filter undo
def undo_all_filters(decompressed, width=1600, channels=2, height=1105):
    stride = 1 + width * channels
    result = np.zeros((height, width * channels), dtype=np.uint8)
    prev_row = np.zeros(width * channels, dtype=np.uint8)
    
    for y in range(height):
        s = y * stride
        ft = decompressed[s]
        rd = np.frombuffer(decompressed[s+1:s+stride], dtype=np.uint8).copy()
        
        if ft == 0:  # None
            pass
        elif ft == 1:  # Sub
            for i in range(channels, len(rd)):
                rd[i] = (rd[i] + rd[i - channels]) & 0xFF
        elif ft == 2:  # Up
            rd = (rd.astype(np.uint16) + prev_row.astype(np.uint16)).astype(np.uint8)
        elif ft == 3:  # Average
            for i in range(len(rd)):
                left = rd[i - channels] if i >= channels else 0
                up = prev_row[i]
                rd[i] = (rd[i] + (left + up) // 2) & 0xFF
        elif ft == 4:  # Paeth
            for i in range(len(rd)):
                left = rd[i - channels] if i >= channels else 0
                up = int(prev_row[i])
                ul = int(prev_row[i - channels]) if i >= channels else 0
                p = left + up - ul
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - ul)
                pred = left if (pa <= pb and pa <= pa) else (up if pb <= pc else ul)
                rd[i] = (rd[i] + pred) & 0xFF
        
        result[y] = rd
        prev_row = rd.copy()
    
    return result

print("Decoding pixels (fast)...")
t0 = time.time()
pixel_data = undo_all_filters(decompressed)
print(f"Done in {time.time()-t0:.1f}s")

gray = pixel_data[:, 0::2]
alpha = pixel_data[:, 1::2]
gray_flat = gray.flatten()
alpha_flat = alpha.flatten()
total = len(gray_flat)
print(f"Pixels: {total}, shape: {gray.shape}")

def scan_bytes(flat_data, name, step=1):
    start = time.time()
    n = len(flat_data)
    count = 0
    for offset in range(0, n - 32, step):
        candidate = bytes(flat_data[offset:offset+32])
        if check_key(candidate):
            print(f"*** SOLVED! {name} offset={offset} key={candidate.hex()}")
            return True
        count += 1
        if count % 100000 == 0:
            print(f"  {name}: {count} checked ({time.time()-start:.1f}s)")
    print(f"  {name}: done {count} in {time.time()-start:.1f}s")
    return False

# ============================================================
# BATCH A: GRAYSCALE - 1-bit LSB packing (fast, ~221K candidates each)
# ============================================================
print("\n=== BATCH A: 1-BIT LSB PACKED ===")

for bit in range(8):
    bits = ((gray_flat >> bit) & 1).astype(np.uint8)
    packed = np.packbits(bits)
    if scan_bytes(packed, f"gray_1bit_b{bit}_raster"): exit()

for bit in range(8):
    bits = ((gray_flat[::-1] >> bit) & 1).astype(np.uint8)
    packed = np.packbits(bits)
    if scan_bytes(packed, f"gray_1bit_b{bit}_rev"): exit()

for bit in range(8):
    bits = ((gray.T.flatten() >> bit) & 1).astype(np.uint8)
    packed = np.packbits(bits)
    if scan_bytes(packed, f"gray_1bit_b{bit}_col"): exit()

# Alpha 1-bit LSB
for bit in range(8):
    bits = ((alpha_flat >> bit) & 1).astype(np.uint8)
    packed = np.packbits(bits)
    if scan_bytes(packed, f"alpha_1bit_b{bit}_raster"): exit()

for bit in range(8):
    bits = ((alpha_flat[::-1] >> bit) & 1).astype(np.uint8)
    packed = np.packbits(bits)
    if scan_bytes(packed, f"alpha_1bit_b{bit}_rev"): exit()

for bit in range(8):
    bits = ((alpha.T.flatten() >> bit) & 1).astype(np.uint8)
    packed = np.packbits(bits)
    if scan_bytes(packed, f"alpha_1bit_b{bit}_col"): exit()

# ============================================================
# BATCH B: 2-BIT PACKED (4 pixels per byte, ~442K candidates)
# ============================================================
print("\n=== BATCH B: 2-BIT PACKED ===")

for shift in [0, 2, 4, 6]:
    extracted = ((gray_flat >> shift) & 3).astype(np.uint8)
    packed = np.zeros(len(extracted) // 4, dtype=np.uint8)
    for i in range(4):
        packed |= (extracted[i::4].astype(np.uint8) << (6 - 2*i))
    if scan_bytes(packed, f"gray_2bit_s{shift}_raster"): exit()

for shift in [0, 2, 4, 6]:
    extracted = ((gray_flat[::-1] >> shift) & 3).astype(np.uint8)
    packed = np.zeros(len(extracted) // 4, dtype=np.uint8)
    for i in range(4):
        packed |= (extracted[i::4].astype(np.uint8) << (6 - 2*i))
    if scan_bytes(packed, f"gray_2bit_s{shift}_rev"): exit()

for shift in [0, 2, 4, 6]:
    extracted = ((gray.T.flatten() >> shift) & 3).astype(np.uint8)
    packed = np.zeros(len(extracted) // 4, dtype=np.uint8)
    for i in range(4):
        packed |= (extracted[i::4].astype(np.uint8) << (6 - 2*i))
    if scan_bytes(packed, f"gray_2bit_s{shift}_col"): exit()

# Alpha 2-bit
for shift in [0, 2, 4, 6]:
    extracted = ((alpha_flat >> shift) & 3).astype(np.uint8)
    packed = np.zeros(len(extracted) // 4, dtype=np.uint8)
    for i in range(4):
        packed |= (extracted[i::4].astype(np.uint8) << (6 - 2*i))
    if scan_bytes(packed, f"alpha_2bit_s{shift}_raster"): exit()

# ============================================================
# BATCH C: 4-BIT PACKED (2 pixels per byte)
# ============================================================
print("\n=== BATCH C: 4-BIT PACKED ===")

for shift in [0, 4]:
    extracted = ((gray_flat >> shift) & 0xF).astype(np.uint8)
    packed = np.zeros(len(extracted) // 2, dtype=np.uint8)
    for i in range(2):
        packed |= (extracted[i::2].astype(np.uint8) << (4 - 4*i))
    if scan_bytes(packed, f"gray_4bit_s{shift}_raster"): exit()

for shift in [0, 4]:
    extracted = ((gray_flat[::-1] >> shift) & 0xF).astype(np.uint8)
    packed = np.zeros(len(extracted) // 2, dtype=np.uint8)
    for i in range(2):
        packed |= (extracted[i::2].astype(np.uint8) << (4 - 4*i))
    if scan_bytes(packed, f"gray_4bit_s{shift}_rev"): exit()

for shift in [0, 4]:
    extracted = ((gray.T.flatten() >> shift) & 0xF).astype(np.uint8)
    packed = np.zeros(len(extracted) // 2, dtype=np.uint8)
    for i in range(2):
        packed |= (extracted[i::2].astype(np.uint8) << (4 - 4*i))
    if scan_bytes(packed, f"gray_4bit_s{shift}_col"): exit()

# Alpha 4-bit
for shift in [0, 4]:
    extracted = ((alpha_flat >> shift) & 0xF).astype(np.uint8)
    packed = np.zeros(len(extracted) // 2, dtype=np.uint8)
    for i in range(2):
        packed |= (extracted[i::2].astype(np.uint8) << (4 - 4*i))
    if scan_bytes(packed, f"alpha_4bit_s{shift}_raster"): exit()

print("\nBATCH A-C DONE")
