from PIL import Image
import numpy as np
import struct
import zlib
from ecdsa import SigningKey, SECP256k1
import hashlib

TARGET = "0xff2142e98e09b5344994f9beb9c56c95506b9f17"
secp256k1_order = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141

def check_key(key_bytes):
    if len(key_bytes) != 32:
        return False
    key_int = int.from_bytes(key_bytes, 'big')
    if not (0 < key_int < secp256k1_order):
        return False
    try:
        sk = SigningKey.from_string(key_bytes, curve=SECP256k1)
        vk = sk.get_verifying_key()
        public_key = b'\x04' + vk.to_string()
        keccak = hashlib.sha3_256(public_key).digest()
        address = '0x' + keccak[-20:].hex()
        return address.lower() == TARGET
    except:
        return False

# Load and decode the PNG properly
with open('puzzle_image.png', 'rb') as f:
    raw_data = f.read()

pos = 8
idat_data = b''
while pos < len(raw_data):
    length = struct.unpack('>I', raw_data[pos:pos+4])[0]
    chunk_type = raw_data[pos+4:pos+8]
    chunk_data = raw_data[pos+8:pos+8+length]
    if chunk_type == b'IDAT':
        idat_data += chunk_data
    pos += 12 + length

decompressed = zlib.decompress(idat_data)

def undo_filter(filter_type, row_data, prev_row, bpp=2):
    result = bytearray(len(row_data))
    if filter_type == 0:
        result[:] = row_data
    elif filter_type == 1:
        for i in range(len(row_data)):
            left = result[i - bpp] if i >= bpp else 0
            result[i] = (row_data[i] + left) & 0xFF
    elif filter_type == 2:
        for i in range(len(row_data)):
            up = prev_row[i] if prev_row else 0
            result[i] = (row_data[i] + up) & 0xFF
    elif filter_type == 3:
        for i in range(len(row_data)):
            left = result[i - bpp] if i >= bpp else 0
            up = prev_row[i] if prev_row else 0
            result[i] = (row_data[i] + (left + up) // 2) & 0xFF
    elif filter_type == 4:
        for i in range(len(row_data)):
            left = result[i - bpp] if i >= bpp else 0
            up = prev_row[i] if prev_row else 0
            up_left = prev_row[i - bpp] if prev_row and i >= bpp else 0
            p = left + up - up_left
            pa, pb, pc = abs(p - left), abs(p - up), abs(p - up_left)
            if pa <= pb and pa <= pc:
                predictor = left
            elif pb <= pc:
                predictor = up
            else:
                predictor = up_left
            result[i] = (row_data[i] + predictor) & 0xFF
    return bytes(result)

row_size = 1 + 1600 * 2
all_rows = []
prev_row = None
for row_idx in range(1105):
    start = row_idx * row_size
    filter_byte = decompressed[start]
    row_data = decompressed[start+1:start+row_size]
    actual_row = undo_filter(filter_byte, row_data, prev_row)
    all_rows.append(actual_row)
    prev_row = actual_row

gray_2d = np.zeros((1105, 1600), dtype=np.uint8)
alpha_2d = np.zeros((1105, 1600), dtype=np.uint8)
for row_idx, row_data in enumerate(all_rows):
    for col in range(1600):
        gray_2d[row_idx, col] = row_data[col * 2]
        alpha_2d[row_idx, col] = row_data[col * 2 + 1]

print("=== Method: Hash-based key derivation ===")
print("Trying various ways to derive a key from the image content")

# Method: SHA-512 of different image regions
for name, region in [
    ("full_gray", gray_2d.tobytes()),
    ("full_alpha", alpha_2d.tobytes()),
    ("building_area_gray", gray_2d[300:600, 0:400].tobytes()),
    ("boat_area_gray", gray_2d[400:700, 0:600].tobytes()),
    ("skyline_gray", gray_2d[200:400, 0:400].tobytes()),
    ("water_gray", gray_2d[800:1105, 0:1600].tobytes()),
]:
    for hash_func in [hashlib.sha256, hashlib.sha384, hashlib.sha512, hashlib.sha3_256, hashlib.sha3_512]:
        h = hash_func(region).digest()
        # Take first 32 bytes
        if check_key(h[:32]):
            print(f"  MATCH with {name} + {hash_func.__name__}!")
        # Take last 32 bytes
        if check_key(h[-32:]):
            print(f"  MATCH with {name} + {hash_func.__name__} (last 32)!")
        # Double hash
        h2 = hash_func(h).digest()
        if check_key(h2[:32]):
            print(f"  MATCH with double {hash_func.__name__} on {name}!")
        if check_key(h2[-32:]):
            print(f"  MATCH with double {hash_func.__name__} (last 32) on {name}!")

print("\n=== Method: Pixel value sequences as key bytes ===")
# Try reading specific rows or columns directly
print("Trying specific rows that had unusual patterns...")

# Row 1038 had 64 non-standard values
row_1038 = gray_2d[1038, :]
# Try extracting key from various subsets
non_std = [(i, int(v)) for i, v in enumerate(row_1038) if v not in [0, 255]]
print(f"Row 1038 non-std values: {len(non_std)}")

# Try all possible contiguous subsequences of length 32
for start_idx in range(len(non_std)):
    for end_idx in range(start_idx + 16, min(start_idx + 48, len(non_std) + 1)):
        vals = [v for _, v in non_std[start_idx:end_idx]]
        if len(vals) == 32:
            key = bytes(vals)
            if check_key(key):
                print(f"  MATCH from row 1038 positions {start_idx}-{end_idx}!")
        # Try as nibbles (lower nibble only)
        if len(vals) >= 32:
            hex_str = ''.join(f'{v % 16:x}' for v in vals[:32])
            if len(hex_str) == 64:
                key = bytes.fromhex(hex_str)
                if check_key(key):
                    print(f"  MATCH from row 1038 nibbles {start_idx}-{end_idx}!")

print("\n=== Method: Reading the image in specific patterns ===")
# Try reading in a spiral pattern from the center
h, w = 1105, 1600
cy, cx = h // 2, w // 2
spiral_values = []
r = 0
directions = [(0, 1), (1, 0), (0, -1), (-1, 0)]
dir_idx = 0
steps_in_dir = 1
steps_taken = 0
turns = 0
y, x = cy, cx
visited = set()

while len(spiral_values) < min(h * w, 100000):
    if 0 <= y < h and 0 <= x < w and (y, x) not in visited:
        spiral_values.append(int(gray_2d[y, x]))
        visited.add((y, x))
    
    dy, dx = directions[dir_idx]
    y, x = y + dy, x + dx
    steps_taken += 1
    
    if steps_taken >= steps_in_dir:
        steps_taken = 0
        dir_idx = (dir_idx + 1) % 4
        turns += 1
        if turns % 2 == 0:
            steps_in_dir += 1

# Test different subsets of the spiral
for start in range(0, min(50000, len(spiral_values) - 32), 100):
    key = bytes(spiral_values[start:start + 32])
    if check_key(key):
        print(f"  MATCH from spiral at position {start}!")

# Try reading in Hilbert curve order
# (Too complex to implement fully, but try a simple version)
print("\n=== Method: Row-by-row with different start columns ===")
for start_col in range(0, 100):
    values = []
    for row_idx in range(1105):
        for offset in range(1600):
            col = (start_col + offset) % 1600
            values.append(int(gray_2d[row_idx, col]))
    
    # Test first 32 bytes
    key = bytes(values[:32])
    if check_key(key):
        print(f"  MATCH from row scan with start_col={start_col}!")
    
    # Test with different intervals
    for interval in [1, 2, 3, 5, 7, 11]:
        values_interval = values[::interval]
        if len(values_interval) >= 32:
            key = bytes(values_interval[:32])
            if check_key(key):
                print(f"  MATCH from row scan start_col={start_col}, interval={interval}!")

print("\n=== Method: BIP39 seed phrase approach ===")
# Try the building heights/widths as BIP39 indices
building_heights = [100, 190, 90, 165, 240, 55, 180, 160, 90, 160, 100, 190]
building_widths = [115, 90, 100, 170, 100, 100, 150, 110, 110, 100, 70, 50]

# BIP39 uses 2048 words, so indices should be 0-2047
# Try mapping building sizes to BIP39 indices
print("Building heights:", building_heights)
print("Building widths:", building_widths)

# Try as 2-byte values
for scale in [1, 2, 5, 10, 20]:
    heights_scaled = [h // scale for h in building_heights]
    widths_scaled = [w // scale for w in building_widths]
    
    # Check if all are in BIP39 range
    all_in_range = all(0 <= h < 2048 for h in heights_scaled + widths_scaled)
    if all_in_range:
        print(f"Scale {scale}: heights={heights_scaled}, widths={widths_scaled} (all in BIP39 range)")

# Try building sizes as hex
for label, sizes in [("heights", building_heights), ("widths", building_widths)]:
    hex_str = ''.join(f'{s:02x}' for s in sizes)
    if len(hex_str) >= 64:
        key = bytes.fromhex(hex_str[:64])
    if check_key(key):
        print(f"  MATCH from building {label} as hex!")

print("\nNo match found yet. This puzzle remains unsolved after extensive analysis.")
print("The exact pixel-level encoding the author used is still unknown.")
