import struct, zlib
import numpy as np
import time
from coincurve import PrivateKey
import hashlib

TARGET = bytes.fromhex("ff2142e98e09b5344994f9beb9c56c95506b9f17")
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141

def keccak256(data):
    return hashlib.new('sha3_256', data).digest()

def check_key(key_bytes):
    key_int = int.from_bytes(key_bytes, 'big')
    if key_int == 0 or key_int >= N: return False
    try:
        sk = PrivateKey(key_bytes)
        pub = sk.public_key.format(compressed=False)
        addr = keccak256(pub)[12:]
        return addr == TARGET
    except: return False

# Load and decode image
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
print("Image loaded.")

total_checked = 0
found = False

def scan_bits(flat_data, n_pixels, bit_width, bit_offset, name):
    global total_checked, found
    if found: return
    
    # Extract bits
    mask = ((1 << bit_width) - 1) << bit_offset
    extracted = (flat_data & mask) >> bit_offset
    
    # Pack into bytes: bits_per_byte = 8 // bit_width
    pixels_per_byte = 8 // bit_width
    n_bytes = n_pixels // pixels_per_byte
    
    packed = np.zeros(n_bytes, dtype=np.uint8)
    for i in range(pixels_per_byte):
        shift = 8 - bit_width * (i + 1)
        packed |= (extracted[i::pixels_per_byte].astype(np.uint8) << shift)
    
    # Scan 32-byte windows
    start = time.time()
    for offset in range(0, n_bytes - 32):
        candidate = bytes(packed[offset:offset+32])
        if check_key(candidate):
            print(f"\n*** SOLVED! {name} offset={offset} key={candidate.hex()}")
            found = True
            return
        total_checked += 1
        if total_checked % 200000 == 0:
            print(f"  {name}: {total_checked} total checked, {time.time()-start:.1f}s this scan")
    print(f"  {name} done: {n_bytes-32} candidates in {time.time()-start:.1f}s")

def scan_bytes(flat_data, n_pixels, name):
    global total_checked, found
    if found: return
    
    start = time.time()
    for offset in range(0, n_pixels - 32):
        candidate = bytes(flat_data[offset:offset+32])
        if check_key(candidate):
            print(f"\n*** SOLVED! {name} offset={offset} key={candidate.hex()}")
            found = True
            return
        total_checked += 1
        if total_checked % 200000 == 0:
            print(f"  {name}: {total_checked} total checked, {time.time()-start:.1f}s")
    print(f"  {name} done: {n_pixels-32} candidates in {time.time()-start:.1f}s")

gray_flat = gray.flatten()
alpha_flat = alpha.flatten()
total_pixels = len(gray_flat)

# === GRAYSCALE CHANNEL ===
print("\n=== GRAYSCALE CHANNEL ===")

# 8-bit (full bytes), raster
print("1. Full bytes, raster")
scan_bytes(gray_flat, total_pixels, "gray_8bit_raster")

if not found:
    # 8-bit, reverse raster
    print("2. Full bytes, reverse raster")
    scan_bytes(gray_flat[::-1], total_pixels, "gray_8bit_reverse")

if not found:
    # 8-bit, column-major
    print("3. Full bytes, column-major")
    gray_col = gray.T.flatten()
    scan_bytes(gray_col, len(gray_col), "gray_8bit_column")

if not found:
    # 8-bit, column-major reverse
    print("4. Full bytes, column-major reverse")
    scan_bytes(gray_col[::-1], len(gray_col), "gray_8bit_column_rev")

# 1-bit LSB, all offsets
for bo in range(8):
    if found: break
    print(f"5-{bo}. 1-bit LSB at bit {bo}, raster")
    scan_bits(gray_flat, total_pixels, 1, bo, f"gray_1bit_b{bo}_raster")

if not found:
    for bo in range(8):
        if found: break
        print(f"6-{bo}. 1-bit LSB at bit {bo}, reverse")
        scan_bits(gray_flat[::-1], total_pixels, 1, bo, f"gray_1bit_b{bo}_reverse")

if not found:
    for bo in range(8):
        if found: break
        print(f"7-{bo}. 1-bit LSB at bit {bo}, column")
        scan_bits(gray_col, len(gray_col), 1, bo, f"gray_1bit_b{bo}_col")

# 2-bit LSB
for bo in range(0, 8, 2):
    if found: break
    print(f"8-{bo}. 2-bit LSB at bit {bo}, raster")
    scan_bits(gray_flat, total_pixels, 2, bo, f"gray_2bit_b{bo}_raster")

if not found:
    for bo in range(0, 8, 2):
        if found: break
        print(f"9-{bo}. 2-bit LSB at bit {bo}, column")
        scan_bits(gray_col, len(gray_col), 2, bo, f"gray_2bit_b{bo}_col")

# 4-bit LSB
for bo in [0, 4]:
    if found: break
    print(f"10-{bo}. 4-bit LSB at bit {bo}, raster")
    scan_bits(gray_flat, total_pixels, 4, bo, f"gray_4bit_b{bo}_raster")

if not found:
    for bo in [0, 4]:
        if found: break
        print(f"11-{bo}. 4-bit LSB at bit {bo}, column")
        scan_bits(gray_col, len(gray_col), 4, bo, f"gray_4bit_b{bo}_col")

# === ALPHA CHANNEL ===
print("\n=== ALPHA CHANNEL ===")

if not found:
    print("12. Alpha full bytes, raster")
    scan_bytes(alpha_flat, total_pixels, "alpha_8bit_raster")

if not found:
    print("13. Alpha full bytes, reverse")
    scan_bytes(alpha_flat[::-1], total_pixels, "alpha_8bit_reverse")

if not found:
    print("14. Alpha full bytes, column")
    alpha_col = alpha.T.flatten()
    scan_bytes(alpha_col, len(alpha_col), "alpha_8bit_column")

for bo in range(8):
    if found: break
    print(f"15-{bo}. Alpha 1-bit at bit {bo}, raster")
    scan_bits(alpha_flat, total_pixels, 1, bo, f"alpha_1bit_b{bo}_raster")

if not found:
    for bo in range(8):
        if found: break
        print(f"16-{bo}. Alpha 1-bit at bit {bo}, reverse")
        scan_bits(alpha_flat[::-1], total_pixels, 1, bo, f"alpha_1bit_b{bo}_reverse")

if not found:
    for bo in range(8):
        if found: break
        print(f"17-{bo}. Alpha 1-bit at bit {bo}, column")
        scan_bits(alpha_col, len(alpha_col), 1, bo, f"alpha_1bit_b{bo}_col")

# === COMBINED GRAY+ALPHA ===
print("\n=== COMBINED GRAY+ALPHA ===")

if not found:
    # Interleave gray and alpha bytes
    print("18. Gray+Alpha interleaved bytes, raster")
    combined = np.zeros(total_pixels * 2, dtype=np.uint8)
    combined[0::2] = gray_flat
    combined[1::2] = alpha_flat
    scan_bytes(combined, len(combined), "ga_interleave_raster")

if not found:
    print("19. Gray+Alpha interleaved bytes, reverse")
    scan_bytes(combined[::-1], len(combined), "ga_interleave_reverse")

if not found:
    # Gray XOR alpha
    print("20. Gray XOR Alpha bytes, raster")
    xored = (gray_flat ^ alpha_flat).astype(np.uint8)
    scan_bytes(xored, total_pixels, "ga_xor_raster")

if not found:
    print("21. Gray XOR Alpha bytes, reverse")
    scan_bytes(xored[::-1], total_pixels, "ga_xor_reverse")

if not found:
    print("22. Gray XOR Alpha, column")
    xored_2d = gray ^ alpha
    xored_col = xored_2d.T.flatten()
    scan_bytes(xored_col, len(xored_col), "ga_xor_column")

if not found:
    # Gray AND Alpha
    print("23. Gray AND Alpha, raster")
    anded = (gray_flat & alpha_flat).astype(np.uint8)
    scan_bytes(anded, total_pixels, "ga_and_raster")

if not found:
    # Gray OR Alpha
    print("24. Gray OR Alpha, raster")
    ored = (gray_flat | alpha_flat).astype(np.uint8)
    scan_bytes(ored, total_pixels, "ga_or_raster")

# === SNAKE ORDER ===
if not found:
    print("\n=== SNAKE ORDER ===")
    snake = []
    for ri in range(1105):
        if ri % 2 == 0:
            snake.extend(gray[ri, :])
        else:
            snake.extend(gray[ri, ::-1])
    snake = np.array(snake, dtype=np.uint8)
    print("25. Gray snake order")
    scan_bytes(snake, len(snake), "gray_snake")

if not found:
    # === SNAKE WITH ALPHA ===
    snake_a = []
    for ri in range(1105):
        if ri % 2 == 0:
            snake_a.extend(alpha[ri, :])
        else:
            snake_a.extend(alpha[ri, ::-1])
    snake_a = np.array(snake_a, dtype=np.uint8)
    print("26. Alpha snake order")
    scan_bytes(snake_a, len(snake_a), "alpha_snake")

if not found:
    # === DIAGONAL SCAN ===
    print("27. Diagonal scan")
    diag = []
    for d in range(1105 + 1600 - 1):
        for y in range(max(0, d - 1600 + 1), min(d + 1, 1105)):
            x = d - y
            if 0 <= x < 1600:
                diag.append(int(gray[y, x]))
    diag = np.array(diag, dtype=np.uint8)
    scan_bytes(diag, len(diag), "gray_diagonal")

if not found:
    # === MSB EXTRACTION ===
    print("\n=== MSB-BASED SCANS ===")
    for bit in range(8):
        if found: break
        msb_data = ((gray_flat >> bit) & 1).astype(np.uint8)
        n_b = len(msb_data) // 8
        packed = np.zeros(n_b, dtype=np.uint8)
        for i in range(8):
            packed |= (msb_data[i::8].astype(np.uint8) << (7-i))
        print(f"Gray bit {bit} packed, raster")
        scan_bytes(packed, len(packed), f"gray_bit{bit}_packed")

if not found:
    print("\n=== INVERSE BIT PACKING (LSB first) ===")
    for bit in range(8):
        if found: break
        bit_data = ((gray_flat >> bit) & 1).astype(np.uint8)
        n_b = len(bit_data) // 8
        packed = np.zeros(n_b, dtype=np.uint8)
        for i in range(8):
            packed |= (bit_data[i::8].astype(np.uint8) << i)
        print(f"Gray bit {bit} packed LSB-first, raster")
        scan_bytes(packed, len(packed), f"gray_bit{bit}_lsbfirst")

if not found:
    # === PAIR COMBINATIONS ===
    print("\n=== PAIR COMBINATIONS ===")
    # (gray MSB, gray LSB) as 2-bit values
    for msb_pos in range(4, 8):
        if found: break
        for lsb_pos in range(0, 4):
            if found: break
            msb = (gray_flat >> msb_pos) & 1
            lsb = (gray_flat >> lsb_pos) & 1
            combined_2bit = (msb << 1) | lsb
            n_b = len(combined_2bit) // 4
            packed = np.zeros(n_b, dtype=np.uint8)
            for i in range(4):
                packed |= (combined_2bit[i::4].astype(np.uint8) << (6-2*i))
            if total_checked % 500000 == 0:
                print(f"Gray bits ({msb_pos},{lsb_pos}) packed")
            scan_bytes(packed, len(packed), f"gray_bits_{msb_pos}_{lsb_pos}")

print(f"\n=== ALL DONE. Total checked: {total_checked}. Found: {found} ===")
