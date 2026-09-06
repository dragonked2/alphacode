from PIL import Image, ImageFilter, ImageEnhance, ImageOps
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
    return False

total_tests = 0

# Load image
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

print(f"Tests so far: {total_tests}")

print("\n=== APPROACH: DCT of sailboat region ===")
from scipy import fft as sp_fft

# Sailboat region (from visual inspection of the image)
# The sail is roughly at y:320-600, x:44-360
sail = gray[320:600, 44:360].astype(float)

# Compute 2D DCT
dct = sp_fft.dctn(sail, norm='ortho')
# Look at coefficients - maybe the key is hidden in specific DCT coefficients
# Try extracting from low-frequency coefficients
dct_flat = np.abs(dct).flatten()
# Sort by magnitude, take the unique positions
sorted_idx = np.argsort(dct_flat)[::-1]

# The key might be encoded in the positions of the largest/smallest coefficients
for threshold_pct in [0.001, 0.005, 0.01, 0.02, 0.05]:
    n_coeffs = int(len(sorted_idx) * threshold_pct)
    positions = sorted_idx[:n_coeffs]
    # Use positions as bytes
    pos_bytes = bytes([int(p) % 256 for p in positions[:32]])
    check_and_report(pos_bytes, f"DCT positions top {threshold_pct}")

# Try extracting key from DCT coefficient values
for start in range(0, len(dct_flat) - 32, max(1, len(dct_flat)//10000)):
    vals = dct_flat[start:start+32]
    # Normalize to 0-255
    if np.max(vals) > 0:
        normalized = (vals / np.max(vals) * 255).astype(np.uint8)
        check_and_report(bytes(normalized), f"DCT values at {start}")

print(f"  DCT done. Tests: {total_tests}")

print("\n=== APPROACH: Alpha channel as binary message ===")
alpha_flat = alpha.flatten()
# Get non-255 alpha positions
non_255 = [(i, int(alpha_flat[i])) for i in range(len(alpha_flat)) if alpha_flat[i] != 255]
print(f"  Non-255 alpha pixels: {len(non_255)}")

if len(non_255) >= 256:
    # Try reading alpha values as binary (above/below median)
    alpha_vals = [v for _, v in non_255]
    median_val = np.median(alpha_vals)
    
    # Try different thresholds
    for thresh in range(225, 255):
        bits = [1 if v > thresh else 0 for v in alpha_vals]
        # Convert to bytes
        for start in range(0, len(bits) - 256, 1):
            key_bytes = bytearray()
            for i in range(32):
                byte_val = 0
                for j in range(8):
                    byte_val = (byte_val << 1) | bits[start + i*8 + j]
                key_bytes.append(byte_val)
            check_and_report(bytes(key_bytes), f"Alpha binary thresh={thresh} start={start}")

print(f"  Alpha binary done. Tests: {total_tests}")

print("\n=== APPROACH: Pixel values read as text on the boat hull ===")
# The boat hull is the lower part of the sailboat
# Roughly y:550-700, x:44-360
hull = gray[550:700, 44:360]
hull_flat = hull.flatten()

# Look for ASCII hex chars (0-9, a-f, A-F) in the hull
ascii_vals = [int(v) for v in hull_flat if 48 <= int(v) <= 102]
print(f"  ASCII hex-range values in hull: {len(ascii_vals)}")
if len(ascii_vals) >= 64:
    # Filter to only valid hex chars
    valid_hex = []
    for v in ascii_vals:
        c = chr(v)
        if c in '0123456789abcdefABCDEF':
            valid_hex.append(c)
    print(f"  Valid hex chars: {len(valid_hex)}")
    if len(valid_hex) >= 64:
        hex_str = ''.join(valid_hex[:64])
        try:
            key = bytes.fromhex(hex_str)
            check_and_report(key, f"Hull ASCII hex: {hex_str[:20]}...")
        except:
            pass

print("\n=== APPROACH: Precise building edge detection ===")
from PIL import Image as PILImage

# Use edge detection on the full image
img = PILImage.fromarray(gray)
edges = img.filter(ImageFilter.FIND_EDGES)
edge_arr = np.array(edges)

# Find horizontal edges (building tops)
for row in range(0, 400):
    edge_row = edge_arr[row, :]
    nonzero = np.where(edge_row > 30)[0]
    if len(nonzero) > 0:
        # These are edge positions - could encode data
        pass

# Try reading the first 64 nonzero edge positions as the key
edge_flat = edge_arr.flatten()
nonzero_edges = np.where(edge_flat > 30)[0]
print(f"  Edge pixels (val > 30): {len(nonzero_edges)}")
if len(nonzero_edges) >= 32:
    key = bytes(nonzero_edges[:32] % 256)
    check_and_report(key, "First 32 edge positions mod 256")

print(f"  Edge done. Tests: {total_tests}")

print("\n=== APPROACH: Difference between consecutive rows ===")
# Maybe the key is encoded in row-to-row differences
for start_row in range(0, 1105 - 32):
    diffs = []
    for i in range(32):
        row_a = gray[start_row + i, :].astype(int)
        row_b = gray[start_row + i + 1, :].astype(int) if start_row + i + 1 < 1105 else row_a
        diff = np.sum(np.abs(row_a - row_b))
        diffs.append(diff % 256)
    check_and_report(bytes(diffs), f"Row diffs starting at row {start_row}")

print(f"  Row diffs done. Tests: {total_tests}")

print("\n=== APPROACH: Mean value of each column ===")
col_means = np.mean(gray, axis=0)
col_means_int = (col_means * 255 / np.max(col_means)).astype(np.uint8)
for start in range(0, 1600 - 32, 1):
    key = bytes(col_means_int[start:start+32])
    check_and_report(key, f"Column means at {start}")

print(f"  Col means done. Tests: {total_tests}")

print(f"\nTotal tests: {total_tests}")
print("Approaches tried: DCT, alpha binary, hull ASCII, edge detection, row diffs, col means")
