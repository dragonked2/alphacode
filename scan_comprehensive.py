import struct, zlib
import numpy as np
import time
from coincurve import PrivateKey
import hashlib

TARGET = bytes.fromhex("ff2142e98e09b5344994f9beb9c56c95506b9f17")
ORDER = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141

def check_key(key_bytes):
    key_int = int.from_bytes(key_bytes, 'big')
    if key_int == 0 or key_int >= ORDER: return False
    try:
        sk = PrivateKey(key_bytes)
        pub = sk.public_key.format(compressed=False)
        addr = hashlib.new('sha3_256', pub).digest()[12:]
        return addr == TARGET
    except: return False

# Load and decode PNG
print("Loading image...")
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

rows_data = []
pr = None
W = 1600
CH = 2
for ri in range(1105):
    s = ri * (1 + W * CH)
    ft = decompressed[s]
    rd = bytearray(decompressed[s+1:s+1+W*CH])
    if ft == 0:
        pass
    elif ft == 1:
        for i in range(CH, len(rd)):
            rd[i] = (rd[i] + rd[i-CH]) & 0xFF
    elif ft == 2:
        if pr:
            for i in range(len(rd)):
                rd[i] = (rd[i] + pr[i]) & 0xFF
    elif ft == 4:
        for i in range(len(rd)):
            l = rd[i-CH] if i >= CH else 0
            u = pr[i] if pr else 0
            ul = pr[i-CH] if pr and i >= CH else 0
            p = l + u - ul
            pa, pb, pc = abs(p - l), abs(p - u), abs(p - ul)
            if pa <= pb and pa <= pc: pred = l
            elif pb <= pc: pred = u
            else: pred = ul
            rd[i] = (rd[i] + pred) & 0xFF
    rows_data.append(bytes(rd))
    pr = rows_data[-1]

print("Building arrays...")
gray_flat = np.empty(1105 * 1600, dtype=np.uint8)
alpha_flat = np.empty(1105 * 1600, dtype=np.uint8)
for ri, rd in enumerate(rows_data):
    for c in range(1600):
        gray_flat[ri*1600+c] = rd[c*2]
        alpha_flat[ri*1600+c] = rd[c*2+1]
print(f"Image loaded. {len(gray_flat)} pixels")

def scan_1bit_packed(flat_data, bit, reverse=False, column=False, label=""):
    if column:
        # Reconstruct 2D, transpose, flatten
        data_2d = flat_data.reshape(1105, 1600)
        flat_data = data_2d.T.flatten()
    if reverse:
        flat_data = flat_data[::-1]
    
    bits = ((flat_data >> bit) & 1).astype(np.uint8)
    packed = np.packbits(bits)
    n = len(packed)
    
    start = time.time()
    for offset in range(0, n - 32):
        candidate = bytes(packed[offset:offset+32])
        if check_key(candidate):
            print(f"*** SOLVED! {label} bit={offset} key={candidate.hex()}")
            return True
        if (offset+1) % 100000 == 0:
            print(f"  {label}: {offset+1}/{n-32} ({time.time()-start:.1f}s)")
    print(f"  {label}: done {n-32} in {time.time()-start:.1f}s")
    return False

def scan_packed(flat_data, bits_per_pixel, shift, reverse=False, column=False, lsb_first=False, label=""):
    if column:
        data_2d = flat_data.reshape(1105, 1600)
        flat_data = data_2d.T.flatten()
    if reverse:
        flat_data = flat_data[::-1]
    
    mask = ((1 << bits_per_pixel) - 1) << shift
    extracted = ((flat_data & mask) >> shift).astype(np.uint8)
    
    pixels_per_byte = 8 // bits_per_pixel
    n_bytes = len(extracted) // pixels_per_byte
    
    packed = np.zeros(n_bytes, dtype=np.uint8)
    if lsb_first:
        for i in range(pixels_per_byte):
            packed |= (extracted[i::pixels_per_byte].astype(np.uint8) << (bits_per_pixel * i))
    else:
        for i in range(pixels_per_byte):
            packed |= (extracted[i::pixels_per_byte].astype(np.uint8) << (8 - bits_per_pixel * (i + 1)))
    
    n = len(packed)
    start = time.time()
    for offset in range(0, n - 32):
        candidate = bytes(packed[offset:offset+32])
        if check_key(candidate):
            print(f"*** SOLVED! {label} offset={offset} key={candidate.hex()}")
            return True
        if (offset+1) % 100000 == 0:
            print(f"  {label}: {offset+1}/{n-32} ({time.time()-start:.1f}s)")
    print(f"  {label}: done {n-32} in {time.time()-start:.1f}s")
    return False

# ============================================================
# PHASE 1: 1-bit LSB (8 bits x 3 orders x 2 channels = 48 scans, ~221K each)
# ============================================================
print("\n=== PHASE 1: 1-BIT LSB ===")

# Grayscale
for bit in range(8):
    for order, rev, col in [("raster", False, False), ("reverse", True, False), ("column", False, True)]:
        label = f"gray_1b{bit}_{order}"
        print(f"\n{label}")
        if scan_1bit_packed(gray_flat, bit, reverse=rev, column=col, label=label): exit()
        # Also try MSB-first packing (bit 7 first in each byte)
        # Actually packbits already does MSB-first, try LSB-first
        if scan_packed(gray_flat, 1, bit, reverse=rev, column=col, lsb_first=True, label=label+"_lsbf"): exit()

# Alpha
for bit in range(8):
    for order, rev, col in [("raster", False, False), ("reverse", True, False), ("column", False, True)]:
        label = f"alpha_1b{bit}_{order}"
        print(f"\n{label}")
        if scan_1bit_packed(alpha_flat, bit, reverse=rev, column=col, label=label): exit()
        if scan_packed(alpha_flat, 1, bit, reverse=rev, column=col, lsb_first=True, label=label+"_lsbf"): exit()

# ============================================================
# PHASE 2: 2-BIT PACKED (4 shifts x 3 orders x 2 channels)
# ============================================================
print("\n=== PHASE 2: 2-BIT PACKED ===")

for shift in [0, 2, 4, 6]:
    for order, rev, col in [("raster", False, False), ("reverse", True, False), ("column", False, True)]:
        label = f"gray_2b{shift}_{order}"
        print(f"\n{label}")
        if scan_packed(gray_flat, 2, shift, reverse=rev, column=col, label=label): exit()
        if scan_packed(gray_flat, 2, shift, reverse=rev, column=col, lsb_first=True, label=label+"_lsbf"): exit()

for shift in [0, 2, 4, 6]:
    for order, rev, col in [("raster", False, False), ("reverse", True, False), ("column", False, True)]:
        label = f"alpha_2b{shift}_{order}"
        print(f"\n{label}")
        if scan_packed(alpha_flat, 2, shift, reverse=rev, column=col, label=label): exit()
        if scan_packed(alpha_flat, 2, shift, reverse=rev, column=col, lsb_first=True, label=label+"_lsbf"): exit()

# ============================================================
# PHASE 3: 4-BIT PACKED
# ============================================================
print("\n=== PHASE 3: 4-BIT PACKED ===")

for shift in [0, 4]:
    for order, rev, col in [("raster", False, False), ("reverse", True, False), ("column", False, True)]:
        label = f"gray_4b{shift}_{order}"
        print(f"\n{label}")
        if scan_packed(gray_flat, 4, shift, reverse=rev, column=col, label=label): exit()
        if scan_packed(gray_flat, 4, shift, reverse=rev, column=col, lsb_first=True, label=label+"_lsbf"): exit()

for shift in [0, 4]:
    for order, rev, col in [("raster", False, False), ("reverse", True, False), ("column", False, True)]:
        label = f"alpha_4b{shift}_{order}"
        print(f"\n{label}")
        if scan_packed(alpha_flat, 4, shift, reverse=rev, column=col, label=label): exit()
        if scan_packed(alpha_flat, 4, shift, reverse=rev, column=col, lsb_first=True, label=label+"_lsbf"): exit()

print("\nPHASE 1-3 DONE - NO MATCH")
