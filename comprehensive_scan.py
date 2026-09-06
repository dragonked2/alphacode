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
        return False, None
    key_int = int.from_bytes(key_bytes, 'big')
    if not (0 < key_int < secp256k1_order):
        return False, None
    try:
        sk = SigningKey.from_string(key_bytes, curve=SECP256k1)
        vk = sk.get_verifying_key()
        public_key = b'\x04' + vk.to_string()
        keccak = hashlib.sha3_256(public_key).digest()
        address = '0x' + keccak[-20:].hex()
        return address.lower() == TARGET, address
    except:
        return False, None

# Load the image properly
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

# Build the image as 2D arrays
gray_2d = np.zeros((1105, 1600), dtype=np.uint8)
alpha_2d = np.zeros((1105, 1600), dtype=np.uint8)
for row_idx, row_data in enumerate(all_rows):
    for col in range(1600):
        gray_2d[row_idx, col] = row_data[col * 2]
        alpha_2d[row_idx, col] = row_data[col * 2 + 1]

gray_flat = gray_2d.flatten()
alpha_flat = alpha_2d.flatten()
total_tests = 0

print("=== Comprehensive LSB + MSB scan ===")

# Test ALL combinations of:
# - channel (gray, alpha, gray XOR alpha, gray AND alpha, etc.)
# - bit position (0-7) 
# - bit width (1-8)
# - reading order (left-right, right-left, top-bottom, bottom-top, zigzag, etc.)
# - byte packing (1-8 bits per byte)

channels = {
    'gray': gray_flat,
    'alpha': alpha_flat,
    'gray_xor_alpha': np.bitwise_xor(gray_flat, alpha_flat),
    'gray_and_alpha': np.bitwise_and(gray_flat, alpha_flat),
    'gray_or_alpha': np.bitwise_or(gray_flat, alpha_flat),
    'gray_sub_alpha': np.mod(np.subtract(gray_flat.astype(int), alpha_flat.astype(int)), 256).astype(np.uint8),
    'alpha_sub_gray': np.mod(np.subtract(alpha_flat.astype(int), gray_flat.astype(int)), 256).astype(np.uint8),
}

# Reading orders
def make_indices_raster(h, w):
    return list(range(h * w))

def make_indices_reverse(h, w):
    return list(range(h * w - 1, -1, -1))

def make_indices_cols(h, w):
    result = []
    for x in range(w):
        for y in range(h):
            result.append(y * w + x)
    return result

def make_indices_cols_reverse(h, w):
    result = []
    for x in range(w-1, -1, -1):
        for y in range(h-1, -1, -1):
            result.append(y * w + x)
    return result

orders = {
    'raster': make_indices_raster(1105, 1600),
    'reverse': make_indices_reverse(1105, 1600),
    'columns': make_indices_cols(1105, 1600),
    'columns_rev': make_indices_cols_reverse(1105, 1600),
}

for ch_name, ch_data in channels.items():
    print(f"\nChannel: {ch_name}")
    
    for order_name, order in orders.items():
        reindexed = ch_data[order]
        
        for bit_offset in range(8):
            for bit_count in [1, 2, 4, 8]:
                if bit_offset + bit_count > 8:
                    continue
                
                mask = ((1 << bit_count) - 1) << bit_offset
                extracted = (reindexed & mask) >> bit_offset
                
                bits_per_byte = 8 // bit_count
                n_bytes = len(extracted) // bits_per_byte
                
                if n_bytes < 32:
                    continue
                
                # Test every starting position (with step for speed)
                max_start = min(len(extracted) - 32 * bits_per_byte, 100000)
                for start in range(0, max_start, max(1, max_start // 10000)):
                    key_bytes = bytearray()
                    valid = True
                    for i in range(32):
                        byte_val = 0
                        for j in range(bits_per_byte):
                            idx = start + i * bits_per_byte + j
                            byte_val = (byte_val << bit_count) | int(extracted[idx])
                        key_bytes.append(byte_val)
                    
                    total_tests += 1
                    match, addr = check_key(bytes(key_bytes))
                    if match:
                        print(f"\n*** MATCH! ***")
                        print(f"Channel: {ch_name}, Order: {order_name}")
                        print(f"Bit offset: {bit_offset}, Bit count: {bit_count}")
                        print(f"Start: {start}")
                        print(f"Key: {bytes(key_bytes).hex()}")
                        exit(0)
        
        if total_tests % 1000000 == 0:
            print(f"  Tested {total_tests} candidates ({ch_name}/{order_name})...")

print(f"\nTotal candidates tested: {total_tests}")
print("No match found.")
