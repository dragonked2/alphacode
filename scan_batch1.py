import struct, zlib
import numpy as np
import time
from coincurve import PrivateKey
import hashlib

TARGET = bytes.fromhex("ff2142e98e09b5344994f9beb9c56c95506b9f17")
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
secp_order = N

def check_key(key_bytes):
    key_int = int.from_bytes(key_bytes, 'big')
    if key_int == 0 or key_int >= secp_order: return False
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

def scan_raw(data, name):
    start = time.time()
    n = len(data)
    for offset in range(0, n - 32):
        candidate = bytes(data[offset:offset+32])
        if check_key(candidate):
            print(f"*** SOLVED! {name} offset={offset} key={candidate.hex()}")
            return True
        if (offset+1) % 100000 == 0:
            print(f"  {name}: {offset+1}/{n-32} ({time.time()-start:.1f}s)")
    print(f"  {name}: done ({time.time()-start:.1f}s), {n-32} candidates")
    return False

gray_flat = gray.flatten()
alpha_flat = alpha.flatten()

# Test 1: Gray 8-bit raster
print("\n1. Gray 8-bit raster")
if scan_raw(gray_flat, "gray_8_raster"): exit()

# Test 2: Gray 8-bit reverse
print("\n2. Gray 8-bit reverse")
if scan_raw(gray_flat[::-1], "gray_8_rev"): exit()

# Test 3: Gray 8-bit column
print("\n3. Gray 8-bit column")
gc = gray.T.flatten()
if scan_raw(gc, "gray_8_col"): exit()

# Test 4: Gray 8-bit column reverse
print("\n4. Gray 8-bit column reverse")
if scan_raw(gc[::-1], "gray_8_col_rev"): exit()

# Test 5: Gray snake
print("\n5. Gray snake")
snake = []
for ri in range(1105):
    if ri % 2 == 0:
        snake.extend(gray[ri, :])
    else:
        snake.extend(gray[ri, ::-1])
if scan_raw(np.array(snake, dtype=np.uint8), "gray_snake"): exit()

# Test 6: Gray snake reverse
print("\n6. Gray snake reverse")
if scan_raw(np.array(snake[::-1], dtype=np.uint8), "gray_snake_rev"): exit()

# Test 7: Alpha 8-bit raster
print("\n7. Alpha 8-bit raster")
if scan_raw(alpha_flat, "alpha_8_raster"): exit()

# Test 8: Alpha 8-bit reverse
print("\n9. Alpha 8-bit reverse")
if scan_raw(alpha_flat[::-1], "alpha_8_rev"): exit()

# Test 9: Alpha column
print("\n9. Alpha 8-bit column")
ac = alpha.T.flatten()
if scan_raw(ac, "alpha_8_col"): exit()

# Test 10: Gray XOR alpha
print("\n10. Gray XOR alpha")
xored = (gray_flat ^ alpha_flat).astype(np.uint8)
if scan_raw(xored, "gray_xor_alpha"): exit()

# Test 11: Gray XOR alpha reverse
print("\n11. Gray XOR alpha reverse")
if scan_raw(xored[::-1], "gray_xor_alpha_rev"): exit()

# Test 12: Gray AND alpha
print("\n12. Gray AND alpha")
if scan_raw((gray_flat & alpha_flat).astype(np.uint8), "gray_and_alpha"): exit()

# Test 13: Gray OR alpha
print("\n13. Gray OR alpha")
if scan_raw((gray_flat | alpha_flat).astype(np.uint8), "gray_or_alpha"): exit()

# Test 14: Gray+Alpha interleaved
print("\n14. Gray+Alpha interleaved")
interleaved = np.zeros(len(gray_flat)*2, dtype=np.uint8)
interleaved[0::2] = gray_flat
interleaved[1::2] = alpha_flat
if scan_raw(interleaved, "ga_interleave"): exit()

# Test 15: Gray+Alpha interleaved reverse
print("\n15. Gray+Alpha interleaved reverse")
if scan_raw(interleaved[::-1], "ga_interleave_rev"): exit()

# Test 16: Diagonal
print("\n16. Gray diagonal")
diag = []
for d in range(1105 + 1600 - 1):
    for y in range(max(0, d - 1600 + 1), min(d + 1, 1105)):
        x = d - y
        diag.append(int(gray[y, x]))
if scan_raw(np.array(diag, dtype=np.uint8), "gray_diag"): exit()

print("\nALL 16 TESTS DONE - NO MATCH")
